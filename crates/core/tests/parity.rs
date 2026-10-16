//! Parity test (spec §5.6, §7): the preview emulator against the real Oh My Posh, cell by
//! cell and color by color, for every layout × every set of segments.
//!
//! It runs the pinned binary that `scripts/fetch-resources.ps1` puts in
//! `resources\omp\oh-my-posh.exe` on the config `generate::omp` writes:
//! `oh-my-posh print primary|right|transient --config <file> --shell cmd --terminal-width <n>
//! --pwd <dir> --status <code> --execution-time <ms>`, reads the colors out of what it prints
//! and compares that with `preview::prompt`. Git's states come from throwaway repositories in
//! a temp folder; the user name, the home folder and Oh My Posh's cache folder are set for
//! each run, so nothing of this machine's own setup leaks in and nothing is written outside
//! the temp folder. The host name and the clock are read from Oh My Posh itself.
//!
//! Without the binary the test fails, saying how to get it, unless
//! `CMDSTUDIO_ALLOW_PARITY_SKIP=1` is set: then it prints `SKIPPED` and passes. CI never sets
//! it.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod sgr;

use cmdstudio_core::generate::omp::config_text;
use cmdstudio_core::model::{Layout, Segment, Theme};
use cmdstudio_core::presets;
use cmdstudio_core::preview::{prompt, Clock, GitState, Line, PreviewContext, Prompt};
use sgr::{difference, parse_sgr};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The version the emulator follows (spec §6.4: OMP is pinned).
const PINNED: &str = "31.5.0";
/// Set to `1` to let the test pass on a machine without the binary.
const ALLOW_SKIP: &str = "CMDSTUDIO_ALLOW_PARITY_SKIP";
/// Keeps the child processes from opening console windows (spec §2.2 #7).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Every segment, in the order of the file format.
const ALL: [Segment; 7] = [
    Segment::Path,
    Segment::Git,
    Segment::Time,
    Segment::User,
    Segment::Host,
    Segment::Status,
    Segment::Duration,
];
const LAYOUTS: [Layout; 3] = [Layout::Box, Layout::Line, Layout::Powerline];

/// A folder in the temp folder, removed again when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        let path = std::env::temp_dir().join(format!("cmdstudio-parity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The pinned binary, or `None` when it is missing and skipping is allowed.
fn pinned_omp() -> Option<PathBuf> {
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\..\resources\omp\oh-my-posh.exe");
    if exe.is_file() {
        return Some(exe);
    }
    if std::env::var(ALLOW_SKIP).is_ok_and(|v| v == "1") {
        println!(
            "SKIPPED parity: {} is missing ({ALLOW_SKIP}=1)",
            exe.display()
        );
        return None;
    }
    panic!(
        "the parity test needs Oh My Posh {PINNED} at {}. Run scripts\\fetch-resources.ps1, \
         or set {ALLOW_SKIP}=1 to skip the test on a machine without it.",
        exe.display()
    );
}

/// Starts `cmd` with an environment of its own: nothing of the parent's, apart from where the
/// system's programs are (`PATH`, `SYSTEMROOT`, `WINDIR`). So a `GIT_DIR` from a hook, the real
/// `LOCALAPPDATA` or the developer's git settings never reach the child. Call it before the
/// `.env(..)` calls that set what the child is meant to see.
fn clean_env(cmd: &mut Command) {
    cmd.env_clear();
    for key in ["PATH", "SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
}

/// Runs git with no global or system settings of this machine, and none of its `GIT_*`
/// variables: the repository it works on is `dir`, whatever the test runs inside of.
fn git(home: &Path, dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    clean_env(&mut cmd);
    let out = cmd
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args([
            "-c",
            "user.name=CMD Studio",
            "-c",
            "user.email=parity@example.invalid",
        ])
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("git runs (the parity test needs git on PATH)");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository in `home\name` on `branch` with one commit, then `change` made to it.
fn repo(home: &Path, name: &str, branch: &str, change: &str) -> PathBuf {
    let dir = home.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(home, &dir, &["init", "-q", "-b", branch]);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(home, &dir, &["add", "a.txt"]);
    git(home, &dir, &["commit", "-q", "-m", "parity"]);
    match change {
        "none" => {}
        "edited" => std::fs::write(dir.join("a.txt"), "a\nb\n").unwrap(),
        "staged" => {
            std::fs::write(dir.join("b.txt"), "b\n").unwrap();
            git(home, &dir, &["add", "b.txt"]);
        }
        "untracked" => std::fs::write(dir.join("u.txt"), "u\n").unwrap(),
        other => panic!("no change {other}"),
    }
    dir
}

/// A config of one segment of `kind` with `template`, written once: how the test learns
/// what it cannot set, the host name and the clock.
fn probe(root: &Path, kind: &str, template: &str) -> PathBuf {
    let config = root.join(format!("probe-{kind}.omp.json"));
    let json = serde_json::json!({
        "version": 4,
        "blocks": [{ "type": "prompt", "alignment": "left", "segments": [
            { "type": kind, "style": "plain", "template": template, "options": { "time_format": "15:04" } }
        ] }]
    });
    std::fs::write(&config, json.to_string()).unwrap();
    config
}

/// Where Oh My Posh may run: the folders it reads and writes, and the git states.
struct Stage {
    exe: PathBuf,
    root: TempDir,
    home: PathBuf,
    /// A folder in no repository.
    plain: PathBuf,
    /// The repositories, with the state the prompt shows for each.
    repos: Vec<(PathBuf, GitState)>,
    host: String,
    /// A config of one time segment, which prints the minute Oh My Posh sees.
    clock: PathBuf,
}

impl Stage {
    /// The stage in `root`, a new temp folder (the caller made it first, to check the
    /// binary's version in it before anything else runs).
    fn new(exe: PathBuf, root: TempDir) -> Stage {
        let home = root.0.join("home");
        let plain = home.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let state = |branch: &str, dirty| GitState {
            branch: branch.to_string(),
            dirty,
        };
        let repos = vec![
            (repo(&home, "clean", "main", "none"), state("main", false)),
            (
                repo(&home, "edited", "feature/preview", "edited"),
                state("feature/preview", true),
            ),
            (repo(&home, "staged", "main", "staged"), state("main", true)),
            (
                repo(&home, "untracked", "main", "untracked"),
                state("main", true),
            ),
            (
                repo(&home, "thai", "งาน/ทดสอบ", "none"),
                state("งาน/ทดสอบ", false),
            ),
        ];
        let host = probe(&root.0, "session", "{{ .HostName }}");
        let clock = probe(&root.0, "time", "{{ .CurrentDate | date .Format }}");
        let mut stage = Stage {
            exe,
            root,
            home,
            plain,
            repos,
            host: String::new(),
            clock,
        };
        stage.host = stage.read(&host, "setup");
        stage
    }

    /// What Oh My Posh prints, as plain text, for one of the `probe` configs.
    fn read(&self, config: &Path, worker: &str) -> String {
        let ctx = self.context(&self.plain, None);
        let out = self.run(config, "primary", &ctx, &[], &["--plain"], worker);
        out.trim().to_string()
    }

    /// The sample for a run in `cwd`.
    fn context(&self, cwd: &Path, git: Option<GitState>) -> PreviewContext {
        PreviewContext {
            user: "tester".to_string(),
            host: self.host.clone(),
            home: self.home.to_string_lossy().into_owned(),
            cwd: cwd.to_string_lossy().into_owned(),
            clock: Clock {
                year: 2026,
                month: 10,
                day: 9,
                hour: 0,
                minute: 0,
            },
            git,
            exit_code: 0,
            duration_ms: 0,
            width: 80,
            windows_terminal: true,
            command: String::new(),
        }
    }

    /// One run of Oh My Posh. The child's `HOME` and `USERPROFILE` are `ctx.home`, and its
    /// `USER` and `USERNAME` are `ctx.user`, unless `env` sets one of them (or another variable)
    /// to something else. `worker` names the cache folder, so parallel runs never share one.
    fn run(
        &self,
        config: &Path,
        kind: &str,
        ctx: &PreviewContext,
        env: &[(&str, String)],
        extra: &[&str],
        worker: &str,
    ) -> String {
        let cache = self.root.0.join("cache").join(worker);
        std::fs::create_dir_all(&cache).unwrap();
        let mut cmd = Command::new(&self.exe);
        clean_env(&mut cmd);
        cmd.args(["print", kind, "--shell", "cmd", "--config"])
            .arg(config)
            .arg("--pwd")
            .arg(&ctx.cwd)
            .args(["--terminal-width", &ctx.width.to_string()])
            .args(["--status", &ctx.exit_code.to_string()])
            .args(["--execution-time", &ctx.duration_ms.to_string()])
            .args(extra)
            .env("USERPROFILE", &ctx.home)
            .env("HOME", &ctx.home)
            .env("USER", &ctx.user)
            .env("USERNAME", &ctx.user)
            .env("LOCALAPPDATA", &cache)
            .env("POSH_SESSION_ID", format!("parity-{worker}"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("TEMP", &self.root.0)
            .env("TMP", &self.root.0)
            .creation_flags(CREATE_NO_WINDOW);
        // a later `env` call for the same name replaces the value
        for (key, value) in env {
            cmd.env(key, value);
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "oh-my-posh print {kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

/// The version `exe` reports, run like every other Oh My Posh run of this test: a cleared
/// environment, with the home folder, the cache folder and the temp folders inside `root`, so
/// not even `oh-my-posh version` reads or writes anything of this machine's own.
fn omp_version(exe: &Path, root: &Path) -> String {
    let home = root.join("home-version");
    let cache = root.join("cache").join("version");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    let mut cmd = Command::new(exe);
    clean_env(&mut cmd);
    let out = cmd
        .arg("version")
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("LOCALAPPDATA", &cache)
        .env("POSH_SESSION_ID", "parity-version")
        .env("TEMP", root)
        .env("TMP", root)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// One comparison: a theme, the sample, and which prompts to print.
struct Case {
    what: String,
    theme: Theme,
    config: PathBuf,
    ctx: PreviewContext,
    /// What the child's environment holds besides what [`Stage::run`] derives from `ctx`.
    env: Vec<(&'static str, String)>,
    /// `print right` and `print transient` too, not only `print primary`.
    all_three: bool,
}

/// Builds the cases, writing one config file for each.
struct Cases<'a> {
    stage: &'a Stage,
    list: Vec<Case>,
}

impl Cases<'_> {
    fn add(&mut self, what: String, theme: Theme, ctx: PreviewContext, all_three: bool) {
        self.add_with_env(what, theme, ctx, Vec::new(), all_three);
    }

    /// [`Cases::add`], with `env` set in the child's environment on top of what `ctx` gives.
    fn add_with_env(
        &mut self,
        what: String,
        theme: Theme,
        ctx: PreviewContext,
        env: Vec<(&'static str, String)>,
        all_three: bool,
    ) {
        theme.validate().unwrap();
        let config = self
            .stage
            .root
            .0
            .join(format!("{}.omp.json", self.list.len()));
        std::fs::write(&config, config_text(&theme).unwrap()).unwrap();
        self.list.push(Case {
            what,
            theme,
            config,
            ctx,
            env,
            all_three,
        });
    }
}

/// The Terracotta preset with this layout and these segments.
fn terracotta(layout: Layout, segments: &[Segment]) -> Theme {
    let mut theme = presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1;
    theme.prompt.layout = layout;
    theme.prompt.segments = segments.to_vec();
    theme
}

/// Every subset of `ALL`, each in the order of `ALL`.
fn subsets() -> Vec<Vec<Segment>> {
    (0u32..1 << ALL.len())
        .map(|mask| {
            ALL.iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, s)| *s)
                .collect()
        })
        .collect()
}

fn all_cases(stage: &Stage) -> Vec<Case> {
    let mut cases = Cases {
        stage,
        list: Vec::new(),
    };
    // the git states: none, and each repository
    let mut gits: Vec<(PathBuf, Option<GitState>)> = vec![(stage.plain.clone(), None)];
    gits.extend(
        stage
            .repos
            .iter()
            .map(|(dir, state)| (dir.clone(), Some(state.clone()))),
    );
    let with =
        |cwd: &Path, git: Option<GitState>, exit_code: i32, duration_ms: u64| PreviewContext {
            exit_code,
            duration_ms,
            ..stage.context(cwd, git)
        };

    // 1. every layout × every set of segments, the states taking turns; all three prompts
    for layout in LAYOUTS {
        for (i, segments) in subsets().into_iter().enumerate() {
            let (cwd, git) = &gits[i % gits.len()];
            let exit_code = [0, 1][i / gits.len() % 2];
            let duration_ms = [1999, 3500][i / (2 * gits.len()) % 2];
            let ctx = with(cwd, git.clone(), exit_code, duration_ms);
            let what =
                format!("{layout:?} {segments:?} git={git:?} exit={exit_code} ms={duration_ms}");
            cases.add(what, terracotta(layout, &segments), ctx, true);
        }
    }
    // 2. orders that are not the file format's, in every state
    let orders: [&[Segment]; 3] = [
        &[
            Segment::Time,
            Segment::Status,
            Segment::User,
            Segment::Path,
            Segment::Duration,
            Segment::Host,
            Segment::Git,
        ],
        &[
            Segment::Duration,
            Segment::Status,
            Segment::Host,
            Segment::User,
            Segment::Time,
            Segment::Git,
            Segment::Path,
        ],
        &[Segment::Host, Segment::Git, Segment::Path, Segment::User],
    ];
    for layout in LAYOUTS {
        for order in orders {
            for (cwd, git) in &gits {
                for exit_code in [0, 2] {
                    for duration_ms in [0, 2000] {
                        let ctx = with(cwd, git.clone(), exit_code, duration_ms);
                        let what = format!(
                            "{layout:?} {order:?} git={git:?} exit={exit_code} ms={duration_ms}"
                        );
                        cases.add(what, terracotta(layout, order), ctx, false);
                    }
                }
            }
        }
    }
    // 3. the box's filler at every kind of width: room to spare, just enough (5 cells free),
    //    one short, a left side wider than the window
    let lists: [&[Segment]; 4] = [
        &[],
        &[Segment::Path],
        &[Segment::Path, Segment::Git, Segment::Time],
        &ALL,
    ];
    for list in lists {
        for width in [1, 9, 10, 18, 20, 32, 33, 34, 40, 61, 80, 200] {
            let ctx = PreviewContext {
                width,
                ..with(&stage.repos[0].0, Some(stage.repos[0].1.clone()), 1, 2500)
            };
            cases.add(
                format!("Box {list:?} width {width}"),
                terracotta(Layout::Box, list),
                ctx,
                false,
            );
        }
    }
    // 3b. the same boundary with wide characters in the path (group 3 has an ASCII path
    //     only): Chinese takes 2 cells a character, an emoji 2, a joined family emoji 6 and a
    //     heart with its variation selector 1, so the edge (5 cells free, one short) falls on
    //     other widths. 16 to 36 holds both edges of each path, and the widths 20 and 33.
    for cwd in [
        r"D:\中文\文件".to_string(),
        "D:\\\u{1f4c1}\\\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\\\u{2764}\u{fe0f}".to_string(),
    ] {
        for list in [&[Segment::Path][..], &[Segment::Path, Segment::Time][..]] {
            for width in 16..=36 {
                let ctx = PreviewContext {
                    width,
                    ..with(Path::new(&cwd), None, 0, 0)
                };
                cases.add(
                    format!("Box {list:?} wide path {cwd} width {width}"),
                    terracotta(Layout::Box, list),
                    ctx,
                    false,
                );
            }
        }
    }
    // 4. paths: the home folder itself, under it, elsewhere, a drive's root, Thai letters with
    //    their marks, Chinese, emoji (the filler counts their cells); none of them needs to
    //    exist. Each is (the home Oh My Posh gets as HOME and USERPROFILE alike, the folder it
    //    gets as --pwd).
    let home = stage.home.to_string_lossy().into_owned();
    let same = |cwd: String| (home.clone(), cwd);
    let paths: Vec<(String, String)> = vec![
        same(home.clone()),
        same(format!(r"{home}\projects\cmd-studio")),
        same(home.to_uppercase()),
        same(r"D:\work\x".to_string()),
        same(r"C:\".to_string()),
        same(format!(r"{home}\ผู้ทดสอบ\ที่นี่")),
        same(r"D:\中文\文件".to_string()),
        same(
            "D:\\\u{1f4c1}\\\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\\\u{2764}\u{fe0f}"
                .to_string(),
        ),
        // the home in lower case, as --pwd and as the home
        same(home.to_lowercase()),
        (home.to_lowercase(), home.clone()),
        (home.to_lowercase(), format!(r"{home}\projects\x")),
        // a folder that only begins like the home folder is not under it
        same(format!(r"{home}2\x")),
        same(format!(r"{home}2")),
        // forward slashes, alone and mixed with backslashes
        same(format!("{home}/projects/x")),
        same(format!(r"{home}\projects/x")),
        same("D:/work/x".to_string()),
        // a trailing separator, under the home folder and elsewhere
        same(format!(r"{home}\projects\")),
        same(r"D:\work\".to_string()),
        // a bare drive, and a drive letter in lower case
        same("D:".to_string()),
        same(r"d:\work".to_string()),
        same(format!(r"{home}\a\..\b")),
        same(r"D:\work\..\x".to_string()),
        // a space in a folder's name
        same(format!(r"{home}\my folder\x")),
        same(r"D:\my folder\x".to_string()),
        // non-ASCII letters in the home folder, in another case in --pwd than in the home
        (r"C:\Users\Ünal".to_string(), r"c:\users\ünal\x".to_string()),
        (r"C:\Users\Ünal".to_string(), r"c:\users\ünal".to_string()),
        (r"C:\Users\ünal".to_string(), r"C:\USERS\ÜNAL\x".to_string()),
        // a dotted capital I against a plain i, both ways round
        (r"C:\Users\İsa".to_string(), r"c:\users\isa\x".to_string()),
        (r"C:\Users\isa".to_string(), r"c:\users\İsa\x".to_string()),
        // Oh My Posh lowers both paths as Go does (İ becomes i, the Kelvin sign k) and cuts the
        // folder at the lowered home's length in bytes. İ in the home and in the folder alike:
        // the cut falls in a name, not at a separator, so the full path shows. The Kelvin sign
        // (3 bytes, 1 once lowered) in the home and the folder alike: the cut falls on a
        // separator two bytes early, so the home is found and one folder too many shows,
        // `~\b\x` (the rule predicted it before this case was first run; the real binary
        // agreed)
        (
            r"C:\Users\İbrahim".to_string(),
            r"C:\Users\İbrahim\Documents".to_string(),
        ),
        (
            "C:\\\u{212a}\\b".to_string(),
            "C:\\\u{212a}\\b\\x".to_string(),
        ),
        // a network folder
        same(r"\\server\share\dir".to_string()),
    ];
    let path_layouts = [
        (Layout::Box, &[Segment::Path, Segment::Time][..]),
        (Layout::Line, &[Segment::Path][..]),
        (Layout::Powerline, &[Segment::Path][..]),
    ];
    for (user_profile, cwd) in paths {
        for (layout, list) in path_layouts {
            let ctx = PreviewContext {
                home: user_profile.clone(),
                ..with(Path::new(&cwd), None, 0, 0)
            };
            cases.add(
                format!("{layout:?} path {cwd} home {user_profile}"),
                terracotta(layout, list),
                ctx,
                false,
            );
        }
    }
    // 4b. which folder `~` stands for: Oh My Posh takes HOME; when HOME is empty, HOMEDRIVE
    //     followed by HOMEPATH; when that is empty too, USERPROFILE. Group 4 gives HOME and
    //     USERPROFILE the same folder, so it cannot tell them apart. Here the sample's home is
    //     the folder Oh My Posh should take, another variable names another folder, and --pwd
    //     is under each of the two in turn. None of the folders needs to exist.
    let profile = stage.root.0.join("profile").to_string_lossy().into_owned();
    let (drive, rest) = home
        .split_at_checked(2)
        .filter(|(drive, _)| drive.ends_with(':'))
        .expect("the temp folder is on a drive");
    let homes: [(&str, Vec<(&'static str, String)>); 3] = [
        (
            "HOME",
            vec![("HOME", home.clone()), ("USERPROFILE", profile.clone())],
        ),
        (
            "HOMEDRIVE and HOMEPATH",
            vec![
                ("HOME", String::new()),
                ("HOMEDRIVE", drive.to_string()),
                ("HOMEPATH", rest.to_string()),
                ("USERPROFILE", profile.clone()),
            ],
        ),
        // the cleared environment has no HOMEDRIVE or HOMEPATH
        (
            "USERPROFILE",
            vec![("HOME", String::new()), ("USERPROFILE", home.clone())],
        ),
    ];
    for (which, env) in homes {
        for cwd in [format!(r"{home}\projects\x"), format!(r"{profile}\x")] {
            for (layout, list) in path_layouts {
                let ctx = PreviewContext {
                    home: home.clone(),
                    ..with(Path::new(&cwd), None, 0, 0)
                };
                cases.add_with_env(
                    format!("{layout:?} path {cwd}, the home from {which}"),
                    terracotta(layout, list),
                    ctx,
                    env.clone(),
                    false,
                );
            }
        }
    }
    // 5. durations in the austin style, around the threshold and across units
    for ms in [
        1999, 2000, 2001, 2010, 2100, 59_999, 60_000, 61_500, 3_600_000, 3_661_001, 90_061_001,
    ] {
        for layout in [Layout::Line, Layout::Powerline] {
            let ctx = with(&stage.plain, None, 0, ms);
            cases.add(
                format!("{layout:?} duration {ms}"),
                terracotta(layout, &[Segment::Duration]),
                ctx,
                false,
            );
        }
    }
    // 6. exit codes, negative ones and an NTSTATUS among them
    for code in [1, -1, 2, 127, 255, 9009, -1_073_741_510] {
        let ctx = with(&stage.plain, None, code, 0);
        cases.add(
            format!("Line status {code}"),
            terracotta(Layout::Line, &[Segment::Status]),
            ctx,
            false,
        );
    }
    // 7. user names, alone and with the host
    for user in ["tester", "ผู้ทดสอบ ไทย", "a.b-c"] {
        for list in [
            &[Segment::User][..],
            &[Segment::Host][..],
            &[Segment::User, Segment::Host][..],
        ] {
            let ctx = PreviewContext {
                user: user.to_string(),
                ..with(&stage.plain, None, 0, 0)
            };
            cases.add(
                format!("Powerline {list:?} user {user}"),
                terracotta(Layout::Powerline, list),
                ctx,
                false,
            );
        }
    }
    // 7b. user names Oh My Posh draws as nothing, in every layout: empty (USER and USERNAME are
    //     both empty, as the sample's user is), and only spaces, so the session segment holds
    //     only spaces and is hidden; and a no-break space, which is not a space to Oh My Posh.
    //     The host name cannot be emptied here: Oh My Posh reads it from Windows itself.
    for user in ["", " ", "\u{a0}"] {
        for layout in LAYOUTS {
            for list in [
                &[Segment::User][..],
                &[Segment::Path, Segment::User, Segment::Time][..],
                &[Segment::User, Segment::Host][..],
            ] {
                let ctx = PreviewContext {
                    user: user.to_string(),
                    ..with(&stage.plain, None, 0, 0)
                };
                cases.add(
                    format!("{layout:?} {list:?} user {user:?}"),
                    terracotta(layout, list),
                    ctx,
                    false,
                );
            }
        }
    }
    // 8. every built-in preset in every layout, with every segment showing
    for (key, preset) in presets::builtin().unwrap() {
        for layout in LAYOUTS {
            let mut theme = preset.clone();
            theme.prompt.layout = layout;
            theme.prompt.segments = ALL.to_vec();
            let ctx = with(&stage.repos[1].0, Some(stage.repos[1].1.clone()), 1, 3500);
            cases.add(format!("{key} {layout:?}"), theme, ctx, true);
        }
    }
    cases.list
}

/// What Oh My Posh printed for one case.
struct Printed {
    primary: String,
    right: Option<String>,
    transient: Option<String>,
}

/// Runs one batch of cases between two readings of Oh My Posh's clock, until a batch starts
/// and ends in the same minute. Returns the minute and what was printed.
fn run_batch(stage: &Stage, batch: &[Case], worker: &str) -> (Clock, Vec<Printed>) {
    for _ in 0..5 {
        let before = stage.read(&stage.clock, worker);
        let printed: Vec<Printed> = batch
            .iter()
            .map(|case| {
                let run = |kind| stage.run(&case.config, kind, &case.ctx, &case.env, &[], worker);
                Printed {
                    primary: run("primary"),
                    right: case.all_three.then(|| run("right")),
                    transient: case.all_three.then(|| run("transient")),
                }
            })
            .collect();
        let after = stage.read(&stage.clock, worker);
        if before == after {
            let (hour, minute) = before.split_once(':').unwrap();
            let clock = Clock {
                hour: hour.parse().unwrap(),
                minute: minute.parse().unwrap(),
                ..batch[0].ctx.clock
            };
            return (clock, printed);
        }
    }
    panic!("the clock moved to a new minute during five batches in a row");
}

/// How `case` differs from what was printed, if it does.
fn compare(case: &Case, clock: Clock, printed: &Printed) -> Option<String> {
    let ctx = PreviewContext {
        clock,
        ..case.ctx.clone()
    };
    let want: Prompt = prompt(&case.theme, &ctx);
    let colors = &case.theme.colors;
    let read = |out: &str| parse_sgr(out, colors.foreground, colors.background);
    let mut checks: Vec<(&str, Vec<Line>, &str)> =
        vec![("primary", want.primary.clone(), &printed.primary)];
    if let Some(right) = &printed.right {
        checks.push(("right", vec![want.right.clone()], right));
    }
    if let Some(transient) = &printed.transient {
        checks.push(("transient", vec![want.transient.clone()], transient));
    }
    for (kind, want, out) in checks {
        match read(out) {
            Err(e) => return Some(format!("{}: print {kind}: {e}: {out:?}", case.what)),
            Ok(got) => {
                if let Some(diff) = difference(&want, &got) {
                    return Some(format!("{}: print {kind}: {diff}", case.what));
                }
            }
        }
    }
    None
}

#[test]
fn the_emulator_draws_what_oh_my_posh_prints() {
    let Some(exe) = pinned_omp() else {
        return;
    };
    let root = TempDir::new();
    assert_eq!(
        omp_version(&exe, &root.0),
        PINNED,
        "{} is not the pinned Oh My Posh",
        exe.display()
    );
    let stage = Stage::new(exe, root);
    let cases = all_cases(&stage);
    let batches: Vec<&[Case]> = cases.chunks(16).collect();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for w in 0..workers {
            let (stage, batches, next, failures) = (&stage, &batches, &next, &failures);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(batch) = batches.get(i) else {
                    break;
                };
                let (clock, printed) = run_batch(stage, batch, &w.to_string());
                for (case, out) in batch.iter().zip(&printed) {
                    if let Some(failure) = compare(case, clock, out) {
                        failures.lock().unwrap().push(failure);
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    // the workers finish in any order; sorted, the message is the same on every run
    failures.sort();
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from Oh My Posh {PINNED}; the first ones:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("parity: {} cases match Oh My Posh {PINNED}", cases.len());
}
