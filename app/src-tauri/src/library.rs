//! The theme library (spec §3.1): the twelve presets, embedded in the core; "my themes" as
//! `*.cmdtheme` files in `<data>\themes`, the id inside each file being its identity; the
//! editor's autosaved drafts in `<data>\drafts\<id>.cmdtheme` (spec §3.4); import, preview
//! first, with a new id when the file's id is taken (spec §3.3); export. A preset is never
//! changed in place: "Customize" copies it into my themes.
use crate::backend::{Backend, Machine, PendingImport};
use crate::error::AppError;
use cmdstudio_core::adapters::AdapterError;
use cmdstudio_core::model::{parse_theme, Layout, Target, Theme};
use cmdstudio_core::palette::{Colors, Tone};
use cmdstudio_core::presets;
use cmdstudio_core::state::State;
use cmdstudio_core::text::safe_file_stem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, PoisonError};
use ts_rs::TS;
use uuid::Uuid;

pub const THEME_EXT: &str = "cmdtheme";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ThemeSource {
    /// Built in; customize to change.
    Preset,
    /// A file in `themes\`.
    Mine,
    /// Only a draft in `drafts\`: a new theme that was never saved.
    Draft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThemeSummary {
    pub id: Uuid,
    pub name: String,
    pub tone: Tone,
    pub layout: Layout,
    /// The 23 colors of the theme this summary is of, for the swatches of its card: one
    /// `themes_list` draws every card, with no `theme_get` (and no log line) per card. In the
    /// listing, a theme of mine that has a draft shows the saved theme's colors; the answer of
    /// `theme_save` with `draft: true` is of the draft, so it has the draft's colors.
    pub colors: Colors,
    pub source: ThemeSource,
    /// The theme state.json says is applied (spec §3.1 "ใช้อยู่").
    pub in_use: bool,
    /// A draft of it waits in `drafts\`.
    pub has_draft: bool,
}

/// A file in `themes\`, or a draft in `drafts\` (then `file` is `drafts\<name>`), that is not a
/// theme of the library, and why: a draft that is lost without a word would be a silent change
/// (spec §2.1). The reason is an
/// [`AppError`] like every other the UI shows (spec §5.9): a code, the texts of that row in
/// Thai and English, and the English reason as the `details`. The code is the closest row:
/// `E_THEME_INVALID` for a file that does not parse or validate as a theme, one whose id a
/// preset or another file already has; `E_THEME_NEWER_SCHEMA` for one a newer CMD Studio
/// wrote; `E_MACHINE_READ` for one that could not be read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryProblem {
    pub file: String,
    pub error: AppError,
}

/// The applied theme as state.json records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppliedInfo {
    pub theme_id: Uuid,
    pub theme_name: String,
    pub target: Target,
    #[ts(type = "number")]
    pub applied_at_ms: u64,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Library {
    /// Presets in their fixed order, then my themes by name, then drafts of unsaved themes.
    pub themes: Vec<ThemeSummary>,
    pub applied: Option<AppliedInfo>,
    pub problems: Vec<LibraryProblem>,
    /// state.json could not be read, so `applied` and `in_use` are unknown (doctor D9 says more).
    pub state_problem: Option<AppError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThemeDetail {
    pub theme: Theme,
    pub source: ThemeSource,
    /// The autosaved draft, when there is one and it reads.
    pub draft: Option<Theme>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportPreview {
    /// Stands for the theme read from the file, for `theme_import_confirm`.
    pub token: String,
    pub theme: Theme,
    /// Its id is already in the library: the copy gets a new one (spec §3.3).
    pub id_taken: bool,
    /// A theme of that name is already in the library.
    pub name_taken: bool,
}

fn has_theme_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(THEME_EXT))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn same_file_name(a: &Path, b: &Path) -> bool {
    match (
        a.file_name().and_then(|n| n.to_str()),
        b.file_name().and_then(|n| n.to_str()),
    ) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// The theme files of a folder (`themes` or `drafts`) as read: each file with its theme, and the
/// files that are not themes.
type ThemeFiles = (Vec<(PathBuf, Theme)>, Vec<LibraryProblem>);

fn data_write(e: AdapterError) -> AppError {
    AppError::coded("E_DATA_WRITE", e.to_string())
}

fn no_such_theme(id: Uuid) -> AppError {
    AppError::coded(
        "E_THEME_INVALID",
        format!("there is no theme with the id {id}; it may have been deleted"),
    )
}

impl<M: Machine> Backend<M> {
    fn presets(&self) -> Result<Vec<Theme>, AppError> {
        Ok(presets::builtin()?.into_iter().map(|(_, t)| t).collect())
    }

    /// Holds the library lock (see `Backend::library_lock`) until the guard is dropped. A
    /// command that holds it calls only the `_locked` methods and the readers, never another
    /// command that takes it (the lock is not reentrant); `pending_import` is taken only after
    /// it, never before.
    fn hold_library(&self) -> MutexGuard<'_, ()> {
        self.library_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// A theme file as read: its theme, `None` when the file is gone, or why it is not one, as
    /// the error the UI shows.
    fn read_theme_file(&self, path: &Path) -> Result<Option<Theme>, AppError> {
        let Some(bytes) =
            self.sys().fs.read(path).map_err(|e| {
                AppError::coded("E_MACHINE_READ", format!("could not be read: {e}"))
            })?
        else {
            return Ok(None);
        };
        Ok(Some(parse_theme(&bytes)?))
    }

    /// My themes: every `.cmdtheme` in `themes\`, in file-name order. A file that does not
    /// parse, one whose id another file (or a preset) already has, is a problem, not a theme.
    fn mine(&self) -> Result<ThemeFiles, AppError> {
        let fs = self.sys().fs;
        let dir = self.paths().themes();
        let entries = fs.list_dir(&dir).map_err(|e| {
            AppError::coded(
                "E_MACHINE_READ",
                format!("{} could not be listed: {e}", dir.display()),
            )
        })?;
        let preset_ids: Vec<Uuid> = self.presets()?.iter().map(|t| t.id).collect();
        let mut themes: Vec<(PathBuf, Theme)> = Vec::new();
        let mut problems = Vec::new();
        for entry in entries
            .into_iter()
            .filter(|e| !e.is_dir && has_theme_ext(&e.path))
        {
            let file = name_of(&entry.path);
            let theme = match self.read_theme_file(&entry.path) {
                Ok(Some(theme)) => theme,
                Ok(None) => continue,
                Err(error) => {
                    problems.push(LibraryProblem { file, error });
                    continue;
                }
            };
            if preset_ids.contains(&theme.id) {
                problems.push(LibraryProblem {
                    file,
                    error: AppError::coded(
                        "E_THEME_INVALID",
                        "has the id of a built-in preset; give it a new id or delete it",
                    ),
                });
            } else if let Some((first, _)) = themes.iter().find(|(_, t)| t.id == theme.id) {
                problems.push(LibraryProblem {
                    file,
                    error: AppError::coded(
                        "E_THEME_INVALID",
                        format!(
                            "has the same id as {}, which CMD Studio shows; give this file a \
                             new id or delete it",
                            name_of(first)
                        ),
                    ),
                });
            } else {
                themes.push((entry.path, theme));
            }
        }
        Ok((themes, problems))
    }

    /// Every draft in `drafts\` that reads, in file-name order, and a problem for each that does
    /// not (`drafts\<name>`; the folder itself when it cannot be listed). A draft that cannot be
    /// read, is damaged or comes from a newer CMD Studio is told, not dropped: it may be the
    /// only copy of a theme that was never saved.
    fn drafts(&self) -> ThemeFiles {
        let dir = self.paths().drafts();
        let entries = match self.sys().fs.list_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                let error = AppError::coded(
                    "E_MACHINE_READ",
                    format!("{} could not be listed: {e}", dir.display()),
                );
                return (
                    Vec::new(),
                    vec![LibraryProblem {
                        file: "drafts".to_string(),
                        error,
                    }],
                );
            }
        };
        let mut drafts = Vec::new();
        let mut problems = Vec::new();
        for entry in entries
            .into_iter()
            .filter(|e| !e.is_dir && has_theme_ext(&e.path))
        {
            match self.read_theme_file(&entry.path) {
                Ok(Some(theme)) => drafts.push((entry.path, theme)),
                Ok(None) => {}
                Err(error) => problems.push(LibraryProblem {
                    file: format!(r"drafts\{}", name_of(&entry.path)),
                    error,
                }),
            }
        }
        (drafts, problems)
    }

    /// Drafts in `drafts\` whose id is no theme of the library: new themes never saved.
    fn orphan_drafts(&self, known: &[Uuid]) -> Vec<(PathBuf, Theme)> {
        self.drafts()
            .0
            .into_iter()
            .filter(|(_, theme)| !known.contains(&theme.id))
            .collect()
    }

    pub(crate) fn applied_info(&self) -> (Option<AppliedInfo>, Option<AppError>) {
        match State::load(self.sys().fs, &self.paths().state_file()) {
            Ok(state) => (
                state.applied.map(|a| AppliedInfo {
                    theme_id: a.theme_id,
                    theme_name: a.theme_name,
                    target: a.target,
                    applied_at_ms: a.applied_at_ms,
                    snapshot_id: a.snapshot_id,
                }),
                None,
            ),
            Err(e) => (None, Some(AppError::from(e))),
        }
    }

    fn summary(&self, theme: &Theme, source: ThemeSource, in_use: Option<Uuid>) -> ThemeSummary {
        ThemeSummary {
            id: theme.id,
            name: theme.name.clone(),
            tone: theme.tone,
            layout: theme.prompt.layout,
            colors: theme.colors.clone(),
            source,
            in_use: in_use == Some(theme.id),
            has_draft: self.sys().fs.exists(&self.draft_path(theme.id)),
        }
    }

    pub fn themes_list(&self) -> Result<Library, AppError> {
        let (applied, state_problem) = self.applied_info();
        let in_use = applied.as_ref().map(|a| a.theme_id);
        let mut themes: Vec<ThemeSummary> = self
            .presets()?
            .iter()
            .map(|t| self.summary(t, ThemeSource::Preset, in_use))
            .collect();
        let (mine, mut problems) = self.mine()?;
        let mut mine: Vec<ThemeSummary> = mine
            .iter()
            .map(|(_, t)| self.summary(t, ThemeSource::Mine, in_use))
            .collect();
        mine.sort_by_key(|t| t.name.to_lowercase());
        themes.extend(mine);
        let known: Vec<Uuid> = themes.iter().map(|t| t.id).collect();
        let (drafts, draft_problems) = self.drafts();
        problems.extend(draft_problems);
        let mut drafts: Vec<ThemeSummary> = drafts
            .iter()
            .filter(|(_, t)| !known.contains(&t.id))
            .map(|(_, t)| self.summary(t, ThemeSource::Draft, in_use))
            .collect();
        drafts.sort_by_key(|t| t.name.to_lowercase());
        themes.extend(drafts);
        Ok(Library {
            themes,
            applied,
            problems,
            state_problem,
        })
    }

    /// A theme by id: a preset, one of mine (with its file), or an unsaved draft (with its
    /// file). `E_THEME_INVALID` when there is none.
    pub(crate) fn find_theme(
        &self,
        id: Uuid,
    ) -> Result<(Theme, ThemeSource, Option<PathBuf>), AppError> {
        if let Some(t) = self.presets()?.into_iter().find(|t| t.id == id) {
            return Ok((t, ThemeSource::Preset, None));
        }
        let (mine, _) = self.mine()?;
        if let Some((path, theme)) = mine.into_iter().find(|(_, t)| t.id == id) {
            return Ok((theme, ThemeSource::Mine, Some(path)));
        }
        self.orphan_drafts(&[])
            .into_iter()
            .find(|(_, t)| t.id == id)
            .map(|(path, theme)| (theme, ThemeSource::Draft, Some(path)))
            .ok_or_else(|| no_such_theme(id))
    }

    pub fn theme_get(&self, id: Uuid) -> Result<ThemeDetail, AppError> {
        let (theme, source, _) = self.find_theme(id)?;
        let draft = match source {
            // The draft is the theme itself.
            ThemeSource::Draft => None,
            _ => match self.sys().fs.read(&self.draft_path(id)) {
                Ok(Some(bytes)) => parse_theme(&bytes).ok(),
                _ => None,
            },
        };
        Ok(ThemeDetail {
            theme,
            source,
            draft,
        })
    }

    pub(crate) fn draft_path(&self, id: Uuid) -> PathBuf {
        self.paths().drafts().join(format!("{id}.{THEME_EXT}"))
    }

    /// Saves a theme of mine (`draft: false`: into `themes\`, and its draft goes) or its draft
    /// (`draft: true`: into `drafts\`). A preset is refused: "Customize" copies it first (spec
    /// §3.1). The file of a theme keeps the name it got when the theme was first saved; a new
    /// one is named after the theme (`text::safe_file_stem`), with ` (2)`, ` (3)`… when that name
    /// is taken. What is written is `Theme::to_file_bytes`: clean, within the limits, so a
    /// theme that does not validate is refused here with the field at fault. The summary is of
    /// the theme as written (its name trimmed and cleaned), as the next listing will show it.
    pub fn theme_save(&self, theme: Theme, draft: bool) -> Result<ThemeSummary, AppError> {
        let _held = self.hold_library();
        self.save_locked(theme, draft)
    }

    /// `theme_save` for a caller that holds the library lock.
    fn save_locked(&self, theme: Theme, draft: bool) -> Result<ThemeSummary, AppError> {
        let bytes = theme.to_file_bytes()?;
        // The theme as the file holds it. `to_file_bytes` never writes what `parse_theme`
        // refuses, so this does not fail for a theme that got this far.
        let written = parse_theme(&bytes)?;
        let fs = self.sys().fs;
        if self.presets()?.iter().any(|p| p.id == written.id) {
            return Err(AppError::coded(
                "E_THEME_INVALID",
                format!(
                    "\"{}\" is a built-in preset, which is not changed in place; Customize makes \
                     a copy of it in My themes",
                    written.name
                ),
            ));
        }
        let in_use = self.applied_info().0.map(|a| a.theme_id);
        if draft {
            fs.write_atomic(&self.draft_path(written.id), &bytes)
                .map_err(data_write)?;
            let source = match self.mine()?.0.iter().any(|(_, t)| t.id == written.id) {
                true => ThemeSource::Mine,
                false => ThemeSource::Draft,
            };
            return Ok(self.summary(&written, source, in_use));
        }
        let (mine, _) = self.mine()?;
        let path = match mine.iter().find(|(_, t)| t.id == written.id) {
            Some((path, _)) => path.clone(),
            None => self.new_theme_file(&written.name, &mine),
        };
        fs.write_atomic(&path, &bytes).map_err(data_write)?;
        // The theme is saved; that is what this command was asked for. Its draft is only the
        // editor's autosave of the same theme, so a draft that will not go (a file in use) is
        // not a failure of a save that worked: the next autosave replaces it and the next save
        // takes it away. Reporting an error here would tell the user their theme was not saved.
        let _ = fs.remove_file(&self.draft_path(written.id));
        Ok(self.summary(&written, ThemeSource::Mine, in_use))
    }

    fn new_theme_file(&self, name: &str, mine: &[(PathBuf, Theme)]) -> PathBuf {
        let dir = self.paths().themes();
        let stem = safe_file_stem(name);
        let taken = |candidate: &Path| {
            mine.iter().any(|(p, _)| same_file_name(p, candidate))
                || self.sys().fs.exists(candidate)
        };
        let mut n: u32 = 1;
        loop {
            let file = if n == 1 {
                format!("{stem}.{THEME_EXT}")
            } else {
                format!("{stem} ({n}).{THEME_EXT}")
            };
            let candidate = dir.join(file);
            if !taken(&candidate) {
                return candidate;
            }
            n = n.saturating_add(1);
        }
    }

    /// A copy of a theme with a new id and the given name, saved into my themes (spec §3.1
    /// "Customize": the UI gives "My <name>" or "<ชื่อ> ของฉัน").
    pub fn theme_duplicate(&self, id: Uuid, name: String) -> Result<ThemeSummary, AppError> {
        let _held = self.hold_library();
        let (mut theme, _, _) = self.find_theme(id)?;
        theme.id = Uuid::new_v4();
        theme.name = name;
        self.save_locked(theme, false)
    }

    /// Deletes a theme of mine and its draft, or (`draft_only`) just the draft. A preset is
    /// refused. Deleting the applied theme is allowed: the machine keeps its generated files
    /// until the next apply, and the UI says so first.
    pub fn theme_delete(&self, id: Uuid, draft_only: bool) -> Result<(), AppError> {
        let _held = self.hold_library();
        let fs = self.sys().fs;
        // Found before anything is removed: a preset or an id nobody has is refused with
        // nothing touched, and a theme that is only a draft is still there to be found.
        let found = match draft_only {
            true => None,
            false => Some(self.find_theme(id)?),
        };
        if let Some((theme, ThemeSource::Preset, _)) = &found {
            return Err(AppError::coded(
                "E_THEME_INVALID",
                format!("\"{}\" is a built-in preset and is not deleted", theme.name),
            ));
        }
        let draft = self.draft_path(id);
        if fs.exists(&draft) {
            fs.remove_file(&draft).map_err(data_write)?;
        }
        match found {
            // The file of mine, or the draft that is the theme (already gone when it has the
            // name `draft_path` gives; `remove_file` does not mind).
            Some((_, _, Some(path))) => fs.remove_file(&path).map_err(data_write),
            _ => Ok(()),
        }
    }

    /// Import, step one (spec §3.1): reads and checks the file, and keeps the theme under a
    /// token until `theme_import_confirm`, so the file is read once. Says whether its id or
    /// its name (letter case aside, as file names and the listing's order treat it) is already
    /// in the library. A file that cannot be read (locked, offline) is `E_MACHINE_READ`, not a
    /// theme that is not valid: the user must not throw away a good theme for that.
    pub fn theme_import_preview_from(&self, path: &Path) -> Result<ImportPreview, AppError> {
        let fs = self.sys().fs;
        let bytes = fs
            .read(path)
            .map_err(|e| {
                AppError::coded(
                    "E_MACHINE_READ",
                    format!("{} could not be read: {e}", path.display()),
                )
            })?
            .ok_or_else(|| {
                AppError::coded(
                    "E_THEME_INVALID",
                    format!("{} does not exist", path.display()),
                )
            })?;
        let theme = parse_theme(&bytes)?;
        let known = self.themes_list()?;
        let id_taken = known.themes.iter().any(|t| t.id == theme.id);
        let name = theme.name.to_lowercase();
        let name_taken = known.themes.iter().any(|t| t.name.to_lowercase() == name);
        let token = Uuid::new_v4().to_string();
        let mut pending = self
            .pending_import
            .lock()
            .map_err(|_| AppError::internal("the import lock is poisoned"))?;
        *pending = Some(PendingImport {
            token: token.clone(),
            theme: theme.clone(),
        });
        Ok(ImportPreview {
            token,
            theme,
            id_taken,
            name_taken,
        })
    }

    /// Import, step two: saves the theme the token stands for into my themes, with a new id
    /// when its id is taken (spec §3.3). The token is good once, and is used up only by a save
    /// that worked: when the save fails (a full disk, a file in use) the preview is still there
    /// and the same token can be confirmed again. A later preview replaces an earlier one, so
    /// the earlier token is then refused.
    pub fn theme_import_confirm(&self, token: &str) -> Result<ThemeSummary, AppError> {
        // The library lock first, then `pending_import`, as `Backend::library_lock` says.
        let _held = self.hold_library();
        let mut theme = {
            let pending = self
                .pending_import
                .lock()
                .map_err(|_| AppError::internal("the import lock is poisoned"))?;
            match pending.as_ref() {
                Some(p) if p.token == token => p.theme.clone(),
                _ => {
                    return Err(AppError::coded(
                        "E_THEME_INVALID",
                        "the import preview is no longer there; pick the file again",
                    ));
                }
            }
        };
        if self.themes_list()?.themes.iter().any(|t| t.id == theme.id) {
            theme.id = Uuid::new_v4();
        }
        let saved = self.save_locked(theme, false)?;
        // Used up now. A preview made since (it does not wait for the library lock) has
        // replaced this one, and is left as it is. The theme is saved, so a lock a panicking
        // thread left is used as it is rather than reported.
        let mut pending = self
            .pending_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if pending.as_ref().is_some_and(|p| p.token == token) {
            *pending = None;
        }
        Ok(saved)
    }

    /// The file name the Save dialog offers for an export: the theme's name as a safe stem.
    pub fn export_file_name(&self, id: Uuid) -> Result<String, AppError> {
        let (theme, _, _) = self.find_theme(id)?;
        Ok(format!("{}.{THEME_EXT}", safe_file_stem(&theme.name)))
    }

    /// Writes the theme to the file the user chose, as `Theme::to_file_bytes` writes it.
    pub fn theme_export_to(&self, id: Uuid, path: &Path) -> Result<(), AppError> {
        let (theme, _, _) = self.find_theme(id)?;
        let bytes = theme.to_file_bytes()?;
        self.sys().fs.write_atomic(path, &bytes).map_err(|e| {
            AppError::coded(
                "E_DATA_WRITE",
                format!("{} could not be written: {e}", path.display()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, fake_fs, terracotta_id, DATA};
    use crate::error::meaning;
    use cmdstudio_core::adapters::fake::FakeMachine;

    fn names(lib: &Library) -> Vec<String> {
        lib.themes.iter().map(|t| t.name.clone()).collect()
    }

    fn has_thai(text: &str) -> bool {
        text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
    }

    #[test]
    fn the_library_lists_the_twelve_presets_first_in_their_order() {
        let b = backend();
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 12);
        assert!(lib
            .themes
            .iter()
            .all(|t| t.source == ThemeSource::Preset && !t.in_use && !t.has_draft));
        assert_eq!(lib.themes[0].name, "Terracotta");
        assert_eq!(lib.themes[11].name, "Paper");
        assert!(lib.applied.is_none() && lib.problems.is_empty() && lib.state_problem.is_none());
    }

    #[test]
    fn customize_copies_a_preset_into_my_themes_with_a_new_id_and_a_safe_file() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "My Terracotta".to_string())
            .unwrap();
        assert_ne!(copy.id, terracotta_id());
        assert_eq!(
            (copy.source, copy.name.as_str()),
            (ThemeSource::Mine, "My Terracotta")
        );
        let file = format!(r"{DATA}\themes\My Terracotta.cmdtheme");
        let saved = parse_theme(&b.sys().fs.read(Path::new(&file)).unwrap().unwrap()).unwrap();
        assert_eq!((saved.id, saved.name.as_str()), (copy.id, "My Terracotta"));
        // The same name again gets its own file; a name with characters a file cannot hold is
        // made safe; the themes come after the presets, by name.
        let again = b
            .theme_duplicate(terracotta_id(), "My Terracotta".to_string())
            .unwrap();
        assert!(b.sys().fs.exists(Path::new(&format!(
            r"{DATA}\themes\My Terracotta (2).cmdtheme"
        ))));
        let odd = b
            .theme_duplicate(terracotta_id(), r#"a/b:"c"?"#.to_string())
            .unwrap();
        assert!(b
            .sys()
            .fs
            .exists(Path::new(&format!(r"{DATA}\themes\a_b__c__.cmdtheme"))));
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 15);
        assert_eq!(
            &names(&lib)[12..],
            ["a/b:\"c\"?", "My Terracotta", "My Terracotta"]
        );
        assert_eq!(b.find_theme(again.id).unwrap().1, ThemeSource::Mine);
        assert_eq!(b.find_theme(odd.id).unwrap().1, ThemeSource::Mine);
    }

    #[test]
    fn a_preset_is_not_saved_or_deleted_in_place() {
        let b = backend();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        let e = b.theme_save(preset, false).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(
            e.details.contains("Customize makes a copy"),
            "{}",
            e.details
        );
        assert_eq!(
            b.theme_delete(terracotta_id(), false).unwrap_err().code,
            "E_THEME_INVALID"
        );
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
    }

    #[test]
    fn a_rename_keeps_the_file_and_a_draft_comes_and_goes() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let (mut theme, _, path) = b.find_theme(copy.id).unwrap();
        let path = path.unwrap();
        // A draft with a new name: the file in themes\ is untouched, the draft is listed.
        theme.name = "Mine, renamed".to_string();
        let drafted = b.theme_save(theme.clone(), true).unwrap();
        assert!(drafted.has_draft && drafted.source == ThemeSource::Mine);
        let detail = b.theme_get(copy.id).unwrap();
        assert_eq!(detail.theme.name, "Mine");
        assert_eq!(detail.draft.unwrap().name, "Mine, renamed");
        assert!(b.sys().fs.exists(&b.draft_path(copy.id)));
        // Saving for real keeps the file name and drops the draft.
        let saved = b.theme_save(theme, false).unwrap();
        assert_eq!(
            (saved.name.as_str(), saved.has_draft),
            ("Mine, renamed", false)
        );
        assert!(b.sys().fs.exists(&path));
        assert!(!b.sys().fs.exists(&b.draft_path(copy.id)));
        assert_eq!(b.theme_get(copy.id).unwrap().theme.name, "Mine, renamed");
        // Deleting takes the file and any draft.
        b.theme_save(b.find_theme(copy.id).unwrap().0, true)
            .unwrap();
        b.theme_delete(copy.id, false).unwrap();
        assert!(!b.sys().fs.exists(&path) && !b.sys().fs.exists(&b.draft_path(copy.id)));
        assert_eq!(b.find_theme(copy.id).unwrap_err().code, "E_THEME_INVALID");
    }

    #[test]
    fn a_draft_of_a_theme_never_saved_is_listed_as_a_draft() {
        let b = backend();
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = "Brand new".to_string();
        let summary = b.theme_save(theme.clone(), true).unwrap();
        assert_eq!(
            (summary.source, summary.has_draft),
            (ThemeSource::Draft, true)
        );
        let lib = b.themes_list().unwrap();
        assert_eq!(
            lib.themes.last().map(|t| (t.source, t.name.as_str())),
            Some((ThemeSource::Draft, "Brand new"))
        );
        let detail = b.theme_get(theme.id).unwrap();
        assert_eq!((detail.source, detail.draft), (ThemeSource::Draft, None));
        assert_eq!(detail.theme.name, "Brand new");
        // Discarding the draft is deleting it.
        b.theme_delete(theme.id, true).unwrap();
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        // Saving it for real makes it one of mine.
        let saved = b.theme_save(theme, false).unwrap();
        assert_eq!(saved.source, ThemeSource::Mine);
    }

    #[test]
    fn deleting_a_theme_never_saved_takes_its_draft_and_a_refused_delete_takes_nothing() {
        let b = backend();
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = "Brand new".to_string();
        b.theme_save(theme.clone(), true).unwrap();
        // A theme with only a draft is listed, so Delete on it must work, not say it is gone.
        b.theme_delete(theme.id, false).unwrap();
        assert!(!b.sys().fs.exists(&b.draft_path(theme.id)));
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        // An id nobody has is refused; a draft planted under a preset's id is not taken by a
        // refused delete of the preset.
        assert_eq!(
            b.theme_delete(theme.id, false).unwrap_err().code,
            "E_THEME_INVALID"
        );
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        let planted = b.draft_path(terracotta_id());
        b.sys()
            .fs
            .write_atomic(&planted, &preset.to_file_bytes().unwrap())
            .unwrap();
        assert_eq!(
            b.theme_delete(terracotta_id(), false).unwrap_err().code,
            "E_THEME_INVALID"
        );
        assert!(b.sys().fs.exists(&planted));
    }

    #[test]
    fn a_theme_that_does_not_validate_is_refused_with_its_field() {
        let b = backend();
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = "   ".to_string();
        let e = b.theme_save(theme, true).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(e.details.contains("name"), "{}", e.details);
        assert!(b.themes_list().unwrap().themes.len() == 12);
    }

    #[test]
    fn files_that_are_not_themes_are_problems_not_themes() {
        let b = backend();
        let themes = b.paths().themes();
        b.sys()
            .fs
            .write_atomic(&themes.join("broken.cmdtheme"), b"{ not json")
            .unwrap();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        b.sys()
            .fs
            .write_atomic(
                &themes.join("copy-of-preset.cmdtheme"),
                &preset.to_file_bytes().unwrap(),
            )
            .unwrap();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let (mine, _, path) = b.find_theme(copy.id).unwrap();
        b.sys()
            .fs
            .write_atomic(
                &themes.join("zz-same-id.cmdtheme"),
                &mine.to_file_bytes().unwrap(),
            )
            .unwrap();
        b.sys()
            .fs
            .write_atomic(&themes.join("notes.txt"), b"not a theme")
            .unwrap();
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 13, "{:?}", names(&lib));
        let files: Vec<&str> = lib.problems.iter().map(|p| p.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "broken.cmdtheme",
                "copy-of-preset.cmdtheme",
                "zz-same-id.cmdtheme"
            ]
        );
        // Each problem carries a code of spec §5.9 (F20): all three are a theme file that is
        // not valid, and the reason, in English, is the detail.
        let codes: Vec<&str> = lib.problems.iter().map(|p| p.error.code.as_str()).collect();
        assert_eq!(codes, ["E_THEME_INVALID"; 3]);
        assert!(
            lib.problems[0]
                .error
                .details
                .starts_with("not a valid theme file: "),
            "{}",
            lib.problems[0].error.details
        );
        assert!(
            lib.problems[1].error.details.contains("built-in preset"),
            "{}",
            lib.problems[1].error.details
        );
        assert!(
            lib.problems[2]
                .error
                .details
                .contains(&name_of(&path.unwrap())),
            "{}",
            lib.problems[2].error.details
        );
    }

    #[test]
    fn a_non_theme_files_problem_carries_the_theme_invalid_code_and_both_texts() {
        let b = backend();
        b.sys()
            .fs
            .write_atomic(&b.paths().themes().join("broken.cmdtheme"), b"{ not json")
            .unwrap();
        let lib = b.themes_list().unwrap();
        let [problem] = &lib.problems[..] else {
            panic!("one problem expected: {:?}", lib.problems);
        };
        assert_eq!(problem.file, "broken.cmdtheme");
        let e = &problem.error;
        assert_eq!(e.code, "E_THEME_INVALID");
        // Both languages, from the row of spec §5.9, so a Thai UI never shows English alone.
        assert!(!e.message_th.is_empty() && has_thai(&e.message_th), "{e:?}");
        assert!(
            !e.message_en.is_empty() && !has_thai(&e.message_en),
            "{e:?}"
        );
        assert_eq!(
            meaning("E_THEME_INVALID"),
            Some((e.message_th.as_str(), e.message_en.as_str()))
        );
        assert!(!e.details.is_empty() && !has_thai(&e.details), "{e:?}");
        // On the wire the problem is camelCase with the whole error inside.
        let json = serde_json::to_value(problem).unwrap();
        assert_eq!(json["file"], "broken.cmdtheme");
        assert_eq!(json["error"]["code"], "E_THEME_INVALID");
        assert!(json["error"]["messageTh"].is_string() && json["error"]["messageEn"].is_string());
    }

    #[test]
    fn a_file_from_a_newer_version_is_a_problem_with_the_newer_schema_code() {
        let b = backend();
        b.sys()
            .fs
            .write_atomic(
                &b.paths().themes().join("future.cmdtheme"),
                b"{\"schemaVersion\": 99}",
            )
            .unwrap();
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 12);
        let [problem] = &lib.problems[..] else {
            panic!("one problem expected: {:?}", lib.problems);
        };
        assert_eq!(problem.file, "future.cmdtheme");
        let e = &problem.error;
        assert_eq!(e.code, "E_THEME_NEWER_SCHEMA");
        assert_eq!(
            meaning("E_THEME_NEWER_SCHEMA"),
            Some((e.message_th.as_str(), e.message_en.as_str()))
        );
        assert!(e.details.contains("schemaVersion 99"), "{}", e.details);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_a_problem_with_the_machine_read_code() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let locked = b.paths().themes().join("locked.cmdtheme");
        let (mine, _, _) = b.find_theme(copy.id).unwrap();
        let mut other = mine.clone();
        other.id = Uuid::new_v4();
        other.name = "Locked".to_string();
        b.sys()
            .fs
            .write_atomic(&locked, &other.to_file_bytes().unwrap())
            .unwrap();
        fake_fs(&b).fail_reads_of(&locked);
        // The good theme is listed; the file that could not be read is named, with the read
        // code and its row's texts in both languages.
        let lib = b.themes_list().unwrap();
        assert_eq!(&names(&lib)[12..], ["Mine"]);
        let [problem] = &lib.problems[..] else {
            panic!("one problem expected: {:?}", lib.problems);
        };
        assert_eq!(problem.file, "locked.cmdtheme");
        let e = &problem.error;
        assert_eq!(e.code, "E_MACHINE_READ");
        assert_eq!(
            meaning("E_MACHINE_READ"),
            Some((e.message_th.as_str(), e.message_en.as_str()))
        );
        assert!(e.details.contains("could not be read"), "{}", e.details);
    }

    #[test]
    fn import_previews_first_and_gives_a_taken_id_a_new_one() {
        let b = backend();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        let file = Path::new(r"C:\Users\tester\Downloads\terracotta.cmdtheme");
        b.sys()
            .fs
            .write_atomic(file, &preset.to_file_bytes().unwrap())
            .unwrap();
        let preview = b.theme_import_preview_from(file).unwrap();
        assert!(preview.id_taken && preview.name_taken);
        assert_eq!(preview.theme.name, "Terracotta");
        // Nothing is in the library until the confirm.
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        let added = b.theme_import_confirm(&preview.token).unwrap();
        assert_ne!(added.id, terracotta_id());
        assert_eq!(
            (added.source, added.name.as_str()),
            (ThemeSource::Mine, "Terracotta")
        );
        assert_eq!(b.themes_list().unwrap().themes.len(), 13);
        // A token is good once; a wrong token changes nothing.
        assert_eq!(
            b.theme_import_confirm(&preview.token).unwrap_err().code,
            "E_THEME_INVALID"
        );
        // A file that is not a theme, and one that is not there.
        b.sys()
            .fs
            .write_atomic(file, b"{\"schemaVersion\": 99}")
            .unwrap();
        assert_eq!(
            b.theme_import_preview_from(file).unwrap_err().code,
            "E_THEME_NEWER_SCHEMA"
        );
        assert_eq!(
            b.theme_import_preview_from(Path::new(r"C:\nowhere.cmdtheme"))
                .unwrap_err()
                .code,
            "E_THEME_INVALID"
        );
    }

    #[test]
    fn export_writes_the_theme_as_a_file_the_import_takes_back() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "ธีม ของฉัน".to_string())
            .unwrap();
        assert_eq!(b.export_file_name(copy.id).unwrap(), "ธีม ของฉัน.cmdtheme");
        let out = Path::new(r"C:\Users\tester\Desktop\out.cmdtheme");
        b.theme_export_to(copy.id, out).unwrap();
        let (theme, _, _) = b.find_theme(copy.id).unwrap();
        assert_eq!(
            b.sys().fs.read(out).unwrap().unwrap(),
            theme.to_file_bytes().unwrap()
        );
        let back = b.theme_import_preview_from(out).unwrap();
        assert!(back.id_taken);
        assert_eq!(back.theme, theme);
    }

    /// A theme with a new id and the given name, as a preset with those two changed.
    fn new_theme(b: &Backend<FakeMachine>, name: &str) -> Theme {
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = name.to_string();
        theme
    }

    /// Writes `theme` as a `.cmdtheme` file at `path`.
    fn put(b: &Backend<FakeMachine>, path: &str, theme: &Theme) {
        b.sys()
            .fs
            .write_atomic(Path::new(path), &theme.to_file_bytes().unwrap())
            .unwrap();
    }

    const DOWNLOADS: &str = r"C:\Users\tester\Downloads";

    #[test]
    fn a_file_that_cannot_be_read_is_not_called_an_invalid_theme_on_import() {
        let b = backend();
        let file = format!(r"{DOWNLOADS}\locked.cmdtheme");
        put(&b, &file, &new_theme(&b, "Locked"));
        fake_fs(&b).fail_reads_of(&file);
        // Locked or offline: the read code, so the user does not throw away a good theme.
        let e = b.theme_import_preview_from(Path::new(&file)).unwrap_err();
        assert_eq!(e.code, "E_MACHINE_READ");
        assert_eq!(
            meaning("E_MACHINE_READ"),
            Some((e.message_th.as_str(), e.message_en.as_str()))
        );
        assert!(e.details.contains("could not be read"), "{}", e.details);
        // A file that is not there stays "not valid".
        let gone = b
            .theme_import_preview_from(Path::new(&format!(r"{DOWNLOADS}\gone.cmdtheme")))
            .unwrap_err();
        assert_eq!(gone.code, "E_THEME_INVALID");
        assert!(gone.details.contains("does not exist"), "{}", gone.details);
    }

    #[test]
    fn a_failed_save_keeps_the_import_for_a_retry() {
        let b = backend();
        let file = format!(r"{DOWNLOADS}\fresh.cmdtheme");
        put(&b, &file, &new_theme(&b, "Fresh"));
        let preview = b.theme_import_preview_from(Path::new(&file)).unwrap();
        // The write into themes\ fails (FakeFs can fail the writes to one path).
        let target = PathBuf::from(format!(r"{DATA}\themes\Fresh.cmdtheme"));
        fake_fs(&b).fail_writes_to(&target);
        let e = b.theme_import_confirm(&preview.token).unwrap_err();
        assert_eq!(e.code, "E_DATA_WRITE");
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        // Once writes work, the same token still stands for the theme.
        fake_fs(&b).stop_failing(&target);
        let added = b.theme_import_confirm(&preview.token).unwrap();
        assert_eq!(
            (added.name.as_str(), added.source),
            ("Fresh", ThemeSource::Mine)
        );
        assert_eq!(b.themes_list().unwrap().themes.len(), 13);
        // And a save that worked uses it up.
        assert_eq!(
            b.theme_import_confirm(&preview.token).unwrap_err().code,
            "E_THEME_INVALID"
        );
    }

    #[test]
    fn a_second_preview_replaces_the_first_and_only_its_token_adds_its_theme() {
        let b = backend();
        let (file_a, file_b) = (
            format!(r"{DOWNLOADS}\a.cmdtheme"),
            format!(r"{DOWNLOADS}\b.cmdtheme"),
        );
        put(&b, &file_a, &new_theme(&b, "Theme A"));
        put(&b, &file_b, &new_theme(&b, "Theme B"));
        let first = b.theme_import_preview_from(Path::new(&file_a)).unwrap();
        let second = b.theme_import_preview_from(Path::new(&file_b)).unwrap();
        assert_ne!(first.token, second.token);
        // The first token is refused and changes nothing, not even the second preview.
        let e = b.theme_import_confirm(&first.token).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        let added = b.theme_import_confirm(&second.token).unwrap();
        assert_eq!(added.name, "Theme B");
        assert!(!names(&b.themes_list().unwrap()).contains(&"Theme A".to_string()));
    }

    #[test]
    fn the_previewed_theme_is_the_one_added_even_if_the_file_changes() {
        let b = backend();
        let file = format!(r"{DOWNLOADS}\changing.cmdtheme");
        put(&b, &file, &new_theme(&b, "As previewed"));
        let preview = b.theme_import_preview_from(Path::new(&file)).unwrap();
        // The file is replaced, or goes, between the preview and the confirm.
        put(&b, &file, &new_theme(&b, "Swapped in"));
        let added = b.theme_import_confirm(&preview.token).unwrap();
        assert_eq!(added.name, "As previewed");
        let lib = b.themes_list().unwrap();
        assert_eq!(&names(&lib)[12..], ["As previewed"]);
    }

    #[test]
    fn a_name_that_differs_only_in_case_is_a_taken_name() {
        let b = backend();
        let file = format!(r"{DOWNLOADS}\shout.cmdtheme");
        put(&b, &file, &new_theme(&b, "TERRACOTTA"));
        let preview = b.theme_import_preview_from(Path::new(&file)).unwrap();
        assert!(preview.name_taken && !preview.id_taken);
        put(&b, &file, &new_theme(&b, "Terracotta Two"));
        let preview = b.theme_import_preview_from(Path::new(&file)).unwrap();
        assert!(!preview.name_taken && !preview.id_taken);
    }

    #[test]
    fn bad_drafts_are_problems_named_with_their_folder_not_themes_that_vanish() {
        let b = backend();
        let drafts = b.paths().drafts();
        // A never-saved theme whose draft reads is listed; one whose draft is damaged is not
        // dropped without a word, and neither is the damaged draft of a saved theme.
        let fine = new_theme(&b, "Brand new");
        b.theme_save(fine, true).unwrap();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        b.sys()
            .fs
            .write_atomic(&b.draft_path(copy.id), b"{ not json")
            .unwrap();
        b.sys()
            .fs
            .write_atomic(&drafts.join("future.cmdtheme"), b"{\"schemaVersion\": 99}")
            .unwrap();
        let locked = drafts.join("locked.cmdtheme");
        b.sys()
            .fs
            .write_atomic(&locked, &new_theme(&b, "Locked").to_file_bytes().unwrap())
            .unwrap();
        fake_fs(&b).fail_reads_of(&locked);
        let lib = b.themes_list().unwrap();
        assert_eq!(&names(&lib)[12..], ["Mine", "Brand new"]);
        let mut found: Vec<(String, String)> = lib
            .problems
            .iter()
            .map(|p| (p.file.clone(), p.error.code.clone()))
            .collect();
        found.sort();
        let mut expected = vec![
            (
                format!(r"drafts\{}.cmdtheme", copy.id),
                "E_THEME_INVALID".to_string(),
            ),
            (
                r"drafts\future.cmdtheme".to_string(),
                "E_THEME_NEWER_SCHEMA".to_string(),
            ),
            (
                r"drafts\locked.cmdtheme".to_string(),
                "E_MACHINE_READ".to_string(),
            ),
        ];
        expected.sort();
        assert_eq!(found, expected);
        // Each carries its row's texts in both languages, like a problem of themes\.
        for p in &lib.problems {
            assert_eq!(
                meaning(&p.error.code),
                Some((p.error.message_th.as_str(), p.error.message_en.as_str()))
            );
        }
    }

    #[test]
    fn a_save_that_worked_is_not_a_failure_because_its_draft_would_not_go() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let (mut theme, _, path) = b.find_theme(copy.id).unwrap();
        b.theme_save(theme.clone(), true).unwrap();
        fake_fs(&b).fail_removes_of(b.draft_path(copy.id));
        theme.name = "Mine, saved".to_string();
        let saved = b.theme_save(theme, false).unwrap();
        assert_eq!(saved.name, "Mine, saved");
        let on_disk = parse_theme(&b.sys().fs.read(&path.unwrap()).unwrap().unwrap()).unwrap();
        assert_eq!(on_disk.name, "Mine, saved");
    }

    #[test]
    fn the_summary_of_a_save_is_the_theme_as_it_was_written() {
        let b = backend();
        // The name is trimmed when the theme is written, and the file is named after that.
        let copy = b
            .theme_duplicate(terracotta_id(), "  Padded  ".to_string())
            .unwrap();
        assert_eq!(copy.name, "Padded");
        assert!(b
            .sys()
            .fs
            .exists(Path::new(&format!(r"{DATA}\themes\Padded.cmdtheme"))));
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.iter().find(|t| t.id == copy.id), Some(&copy));
        // The same for a draft.
        let mut draft = new_theme(&b, " Drafted ");
        let drafted = b.theme_save(draft.clone(), true).unwrap();
        assert_eq!(drafted.name, "Drafted");
        draft.name = "\u{1b}[31mRed\u{1b}[0m".to_string();
        assert_eq!(b.theme_save(draft, true).unwrap().name, "Red");
    }

    #[test]
    fn each_summary_carries_its_themes_colors_for_the_swatches_of_its_card() {
        let b = backend();
        // The presets' as the core embeds them, in the listing's order.
        let lib = b.themes_list().unwrap();
        let builtin = presets::builtin().unwrap();
        assert_eq!(lib.themes.len(), builtin.len());
        for (summary, (key, preset)) in lib.themes.iter().zip(&builtin) {
            assert_eq!(
                (summary.id, &summary.colors),
                (preset.id, &preset.colors),
                "{key}"
            );
        }
        // One of mine: the colors as written, in the save's answer and in the listing, where a
        // draft of it does not change them (the card shows the saved theme).
        let mut theme = new_theme(&b, "Mine");
        theme.colors.accent = "#123456".parse().unwrap();
        let saved = b.theme_save(theme.clone(), false).unwrap();
        assert_eq!(saved.colors, theme.colors);
        let mut draft = theme.clone();
        draft.colors.background = "#000000".parse().unwrap();
        let drafted = b.theme_save(draft.clone(), true).unwrap();
        assert_eq!(drafted.colors, draft.colors);
        let listed = |id: Uuid| {
            b.themes_list()
                .unwrap()
                .themes
                .into_iter()
                .find(|t| t.id == id)
                .unwrap()
        };
        let mine = listed(theme.id);
        assert!(mine.has_draft);
        assert_eq!(mine.colors, theme.colors);
        // A theme that is only a draft: the draft's colors.
        let mut fresh = new_theme(&b, "Brand new");
        fresh.colors.frame = "#abcdef".parse().unwrap();
        b.theme_save(fresh.clone(), true).unwrap();
        assert_eq!(listed(fresh.id).colors, fresh.colors);
    }

    #[test]
    fn the_library_lock_is_let_go_after_each_command_and_a_poisoned_one_is_still_used() {
        let b = backend();
        let free = |b: &Backend<FakeMachine>| b.library_lock.try_lock().is_ok();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        assert!(free(&b));
        assert!(b.theme_duplicate(Uuid::new_v4(), "x".to_string()).is_err());
        assert!(free(&b));
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        assert!(b.theme_save(preset, false).is_err());
        assert!(free(&b));
        b.theme_save(b.find_theme(copy.id).unwrap().0, true)
            .unwrap();
        assert!(free(&b));
        assert!(b.theme_delete(terracotta_id(), false).is_err());
        assert!(free(&b));
        let file = format!(r"{DOWNLOADS}\lock.cmdtheme");
        put(&b, &file, &new_theme(&b, "Lock"));
        let preview = b.theme_import_preview_from(Path::new(&file)).unwrap();
        assert!(b.theme_import_confirm("not the token").is_err());
        assert!(free(&b));
        b.theme_import_confirm(&preview.token).unwrap();
        assert!(free(&b));
        b.theme_delete(copy.id, false).unwrap();
        assert!(free(&b));
        // A command that died holding the lock leaves it poisoned; the library goes on.
        let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = b.library_lock.lock().unwrap();
            panic!("a command died holding the library lock");
        }));
        assert!(died.is_err() && b.library_lock.is_poisoned());
        let again = b
            .theme_duplicate(terracotta_id(), "After".to_string())
            .unwrap();
        b.theme_delete(again.id, false).unwrap();
        assert_eq!(b.themes_list().unwrap().themes.len(), 13);
    }

    /// The library on the real disk from several threads, as the IPC commands are
    /// (`spawn_blocking` on one shared backend). `FakeFs` cannot be shared between threads.
    #[cfg(windows)]
    mod on_disk {
        use super::*;
        use cmdstudio_core::adapters::windows::{
            eventlog::WinEventLog,
            fonts::WinFonts,
            fs::WinFs,
            host::WinHost,
            process::WinProcess,
            registry::{RegLocation, WinAutoRun},
            wt::WinWt,
        };
        use cmdstudio_core::adapters::System;
        use std::time::Duration;

        /// A folder of its own under the system temp folder, removed when the test ends, also
        /// when it fails. The tests touch nothing else on this computer.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Self {
                let name = format!("cmdstudio-library-test-{}", Uuid::new_v4().simple());
                Scratch(std::env::temp_dir().join(name))
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// The real file system and adapters that are never called: the library uses only
        /// `Fs`. Their registry places are names no program has, and nothing opens them.
        struct DiskMachine {
            fs: WinFs,
            autorun: WinAutoRun,
            fonts: WinFonts,
            wt: WinWt,
            process: WinProcess,
            host: WinHost,
            event_log: WinEventLog,
        }

        impl DiskMachine {
            fn under(dir: &Path) -> Self {
                let unused = || RegLocation::user(r"Software\CmdStudioLibraryTestNeverOpened");
                DiskMachine {
                    fs: WinFs,
                    autorun: WinAutoRun::at(unused(), unused()),
                    fonts: WinFonts::at(dir.join("fonts"), unused(), false),
                    wt: WinWt::new(dir.join("local")),
                    process: WinProcess::new(Vec::new(), Duration::from_secs(1)),
                    host: WinHost,
                    event_log: WinEventLog::real(),
                }
            }
        }

        impl Machine for DiskMachine {
            fn system(&self) -> System<'_> {
                System {
                    fs: &self.fs,
                    autorun: &self.autorun,
                    fonts: &self.fonts,
                    wt: &self.wt,
                    process: &self.process,
                    host: &self.host,
                    event_log: &self.event_log,
                }
            }
        }

        #[test]
        fn new_themes_of_one_name_from_many_threads_each_get_a_file_of_their_own() {
            const THREADS: usize = 8;
            const EACH: usize = 4;
            let scratch = Scratch::new();
            let (_, now) = crate::backend::testing::clock();
            let b = Backend::new(
                DiskMachine::under(&scratch.0),
                scratch.0.join("data"),
                scratch.0.join("resources"),
                now,
                crate::backend::testing::sample_identity(),
            );
            std::thread::scope(|scope| {
                for _ in 0..THREADS {
                    scope.spawn(|| {
                        for _ in 0..EACH {
                            b.theme_duplicate(terracotta_id(), "Same".to_string())
                                .unwrap();
                        }
                    });
                }
            });
            // Without the lock two threads choose the same free file name, and the later
            // write replaces the other's theme.
            let files = std::fs::read_dir(b.paths().themes()).unwrap().count();
            assert_eq!(files, THREADS * EACH);
            let lib = b.themes_list().unwrap();
            assert_eq!(lib.themes.len(), 12 + THREADS * EACH);
            assert!(lib.problems.is_empty(), "{:?}", lib.problems);
        }
    }
}
