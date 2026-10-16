//! `open_cmd`'s decision (spec §4.2 page 6 "ลองเปิด cmd", §4.3 "เปิด cmd", §4.4): which window the
//! button opens. The decision is made here, over the machine, so it is tested against the core's
//! in-memory adapters; `crate::open` only starts the window it is given.
use super::{Backend, Machine};
use crate::open::CmdLaunch;
use cmdstudio_core::state::State;

impl<M: Machine> Backend<M> {
    /// The window "Open cmd" opens: a Windows Terminal tab with the cmd profile CMD Studio
    /// themed when a theme is applied and that profile is there; a plain cmd window otherwise.
    ///
    /// The profile is the one window that shows the whole theme (colors, font, prompt and
    /// welcome box), for either target: in "every cmd window" its command line is a bare
    /// `cmd.exe`, so AutoRun loads the prompt, and in "Windows Terminal only" its command line
    /// loads Clink itself and a plain cmd window would show no theme at all (spec §5.4). The
    /// profile is there when a Windows Terminal is installed and has a `settings.json`: the
    /// planner writes the profile into the files that exist, and one that was never opened has
    /// none (doctor D6).
    ///
    /// Nothing here can stop the button: a `state.json` that cannot be used (the Home page and
    /// the doctor tell the user about it) means no theme is known, so the window is a plain cmd.
    pub fn cmd_launch(&self) -> CmdLaunch {
        let sys = self.sys();
        let applied = State::load(sys.fs, &self.paths().state_file())
            .is_ok_and(|state| state.applied.is_some());
        let profile_there = sys
            .wt
            .locations()
            .iter()
            .any(|l| l.installed && sys.fs.exists(&l.settings));
        if applied && profile_there {
            CmdLaunch::WindowsTerminalProfile
        } else {
            CmdLaunch::PlainCmd
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::{ApplyRequest, ApplyStatus};
    use crate::backend::testing::{apply_terracotta, backend, plan_and_apply, terracotta_request};
    use cmdstudio_core::adapters::{WtLocation, WtVariant};
    use cmdstudio_core::model::Target;
    use std::path::PathBuf;

    #[test]
    fn nothing_applied_opens_a_plain_cmd() {
        // Windows Terminal is there, but there is no theme to try in it.
        let b = backend();
        assert_eq!(b.cmd_launch(), CmdLaunch::PlainCmd);
    }

    #[test]
    fn a_theme_for_every_cmd_window_opens_the_terminal_profile_when_the_terminal_is_there() {
        let b = backend();
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert_eq!(b.cmd_launch(), CmdLaunch::WindowsTerminalProfile);
    }

    #[test]
    fn a_theme_for_the_terminal_only_opens_the_terminal_profile() {
        let b = backend();
        let (done, _) = plan_and_apply(
            &b,
            ApplyRequest {
                target: Target::Wt,
                ..terracotta_request()
            },
        );
        assert_eq!(done.status, ApplyStatus::Committed);
        assert_eq!(b.cmd_launch(), CmdLaunch::WindowsTerminalProfile);
    }

    #[test]
    fn without_a_terminal_a_plain_cmd_opens_whatever_was_applied() {
        for target in [Target::All, Target::Wt] {
            let mut b = backend();
            let (done, _) = plan_and_apply(
                &b,
                ApplyRequest {
                    target,
                    ..terracotta_request()
                },
            );
            assert_eq!(done.status, ApplyStatus::Committed);
            // The Terminal is uninstalled after the theme was applied.
            b.machine.wt.locations = vec![];
            assert_eq!(b.cmd_launch(), CmdLaunch::PlainCmd, "{target:?}");
        }
    }

    /// The profile is there when the Terminal is installed and its settings.json exists: the
    /// planner writes the cmd profile into the settings files that exist, and a Terminal that
    /// was never opened has none (D6).
    #[test]
    fn the_profile_needs_an_installed_terminal_with_a_settings_file() {
        // (installed, settings.json exists, the profile opens)
        let table = [
            (true, true, true),
            // Installed but never opened: no settings.json, so no profile was written.
            (true, false, false),
            // A folder left behind: the Terminal itself is not there.
            (false, true, false),
            (false, false, false),
        ];
        for (installed, has_settings, opens_profile) in table {
            let mut b = backend();
            let settings = if has_settings {
                PathBuf::from(crate::backend::testing::WT)
            } else {
                PathBuf::from(r"C:\Users\tester\no\such\settings.json")
            };
            b.machine.wt.locations = vec![WtLocation {
                variant: WtVariant::Store,
                installed,
                settings,
            }];
            let (done, _) = apply_terracotta(&b);
            assert_eq!(done.status, ApplyStatus::Committed);
            let want = if opens_profile {
                CmdLaunch::WindowsTerminalProfile
            } else {
                CmdLaunch::PlainCmd
            };
            assert_eq!(
                b.cmd_launch(),
                want,
                "installed {installed}, settings.json {has_settings}"
            );
        }
    }

    #[test]
    fn an_undone_theme_opens_a_plain_cmd_again() {
        let b = backend();
        apply_terracotta(&b);
        assert_eq!(b.cmd_launch(), CmdLaunch::WindowsTerminalProfile);
        b.undo_last().unwrap();
        assert_eq!(b.cmd_launch(), CmdLaunch::PlainCmd);
    }

    /// The button must always give a window: a state file that cannot be used (damaged, or from
    /// a newer CMD Studio) is no reason to refuse one.
    #[test]
    fn a_state_file_that_cannot_be_used_still_opens_a_plain_cmd() {
        let b = backend();
        apply_terracotta(&b);
        assert_eq!(b.cmd_launch(), CmdLaunch::WindowsTerminalProfile);
        let state = b.paths().state_file();
        b.sys().fs.write_atomic(&state, b"{ not json").unwrap();
        assert_eq!(b.cmd_launch(), CmdLaunch::PlainCmd);
        b.sys()
            .fs
            .write_atomic(&state, br#"{"version": 99}"#)
            .unwrap();
        assert_eq!(b.cmd_launch(), CmdLaunch::PlainCmd);
    }
}
