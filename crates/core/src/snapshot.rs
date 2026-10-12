//! Restore points on disk (spec §5.3, §5.5):
//! `<data>\snapshots\<id>\manifest.json`, `journal.json` and
//! `payload\<op index>.bin`, one file per prior state that existed. Keeps the
//! 20 newest plus the first ever.
//!
//! Failing to read a restore point is `E_SNAPSHOT_READ`; failing to write or delete one is
//! `E_SNAPSHOT_WRITE`. A restore point that cannot be read must never stop a later command,
//! `--cleanup` or doctor: [`SnapshotStore::unreadable`] lists the ones that cannot, so that a
//! person is told once, and [`SnapshotStore::set_aside`] renames the journal of each to
//! `journal.unreadable.json`, which ends the trouble and keeps the file for a look. Until then
//! an undo does not go past one: see [`SnapshotStore::latest_undoable`].
use crate::adapters::Fs;
use crate::model::Target;
use crate::plan::Op;
use crate::txn::{Failure, Journal, JournalStore, Snapshot, Stage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How many recent restore points to keep, besides the first ever.
pub const KEEP: usize = 20;
/// A restore point could not be written or deleted.
const SNAPSHOT_WRITE: &str = "E_SNAPSHOT_WRITE";
/// A restore point could not be read: a file is missing, locked or damaged.
const SNAPSHOT_READ: &str = "E_SNAPSHOT_READ";
const JOURNAL: &str = "journal.json";
const INCOMPLETE: &str = "incomplete.json";
/// What `journal.json` is renamed to when its restore point is set aside.
const JOURNAL_ASIDE: &str = "journal.unreadable.json";
/// Written when a restore point is set aside that had no `journal.json` to rename. It is not
/// `journal.unreadable.json`: that name means a journal existed, so the transaction ran; this
/// one means it never got as far as a journal.
const NO_JOURNAL: &str = "journal.missing.json";
const NO_JOURNAL_NOTE: &[u8] =
    b"{\n  \"note\": \"journal.json was missing when this restore point was set aside\"\n}\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotKind {
    Apply,
    Undo,
    RevertAll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRef {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub kind: SnapshotKind,
    pub created_ms: u64,
    pub theme: Option<ThemeRef>,
    pub target: Option<Target>,
    /// The plan's English summary lines.
    pub summary: Vec<String>,
    /// For an undo: the apply it reverses.
    pub undoes: Option<String>,
    /// For an apply: the undo or revert-all that reversed it.
    pub undone_by: Option<String>,
}

/// `journal.json`: the txn journal, with each prior state moved out to a
/// payload file. `payload: None` means the target did not exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    id: String,
    ops: Vec<Op>,
    snapshots: Vec<PayloadRef>,
    stage: Stage,
}

impl JournalFile {
    /// The transaction ended, as [`Journal::is_finished`] says, judged from the stage alone: the
    /// payload files are not looked at.
    fn is_finished(&self) -> bool {
        Journal {
            id: String::new(),
            ops: Vec::new(),
            snapshots: Vec::new(),
            stage: self.stage.clone(),
        }
        .is_finished()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PayloadRef {
    op_index: usize,
    payload: Option<String>,
}

/// The one name `journal.json` may give the payload of op `op_index`, relative to the restore
/// point. A person can edit the journal, so a reader checks the name instead of trusting it.
fn payload_name(op_index: usize) -> String {
    format!("payload/{op_index}.bin")
}

/// A sortable, unique id: 13-digit milliseconds, then 8 hex characters.
pub fn new_id(now_ms: u64) -> String {
    let tail: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    format!("{now_ms:013}-{tail}")
}

fn write_fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: SNAPSHOT_WRITE,
        message: e.to_string(),
    }
}

fn read_fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: SNAPSHOT_READ,
        message: e.to_string(),
    }
}

/// What is wrong with a restore point that cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnreadableKind {
    /// A file the restore point needs is not there: its manifest, its journal, or a payload the
    /// journal names. It will not come back.
    Missing,
    /// The files are there but not valid: JSON of the wrong shape, or a journal that names a
    /// payload file it may not. It will not mend itself.
    Damaged,
    /// The read itself failed: the file is there, and a lock, a scan or a permission kept it
    /// from being read now. This may pass.
    CouldNotRead,
}

/// A restore point that cannot be used, and why (see [`SnapshotStore::unreadable`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    pub id: String,
    pub kind: UnreadableKind,
    /// The first file that could not be used, with the reason: always `E_SNAPSHOT_READ`.
    pub failure: Failure,
}

/// A read that failed, and which way.
struct ReadError {
    kind: UnreadableKind,
    failure: Failure,
}

impl ReadError {
    fn new(kind: UnreadableKind, e: impl std::fmt::Display) -> Self {
        ReadError {
            kind,
            failure: read_fail(e),
        }
    }
}

#[derive(Clone, Copy)]
pub struct SnapshotStore<'a> {
    fs: &'a dyn Fs,
    root: &'a Path,
}

impl<'a> SnapshotStore<'a> {
    pub fn new(fs: &'a dyn Fs, root: &'a Path) -> Self {
        SnapshotStore { fs, root }
    }

    /// Ids of every restore point, oldest first.
    pub fn ids(&self) -> Result<Vec<String>, Failure> {
        let mut ids: Vec<String> = self
            .fs
            .list_dir(self.root)
            .map_err(read_fail)?
            .into_iter()
            .filter(|e| e.is_dir)
            .filter_map(|e| {
                e.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            })
            .collect();
        ids.sort();
        Ok(ids)
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// A file of a restore point. A missing file is an error: it is never read as "empty".
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, ReadError> {
        match self.fs.read(path) {
            Err(e) => Err(ReadError::new(UnreadableKind::CouldNotRead, e)),
            Ok(None) => Err(ReadError::new(
                UnreadableKind::Missing,
                format!("{} is missing", path.display()),
            )),
            Ok(Some(bytes)) => Ok(bytes),
        }
    }

    fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<T, ReadError> {
        serde_json::from_slice(&self.read_file(path)?).map_err(|e| {
            ReadError::new(
                UnreadableKind::Damaged,
                format!("{} is damaged: {e}", path.display()),
            )
        })
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), Failure> {
        let mut bytes = serde_json::to_vec_pretty(value).map_err(write_fail)?;
        bytes.push(b'\n');
        self.fs.write_atomic(path, &bytes).map_err(write_fail)
    }

    pub fn manifest(&self, id: &str) -> Result<Manifest, Failure> {
        self.read_manifest(id).map_err(|e| e.failure)
    }

    fn read_manifest(&self, id: &str) -> Result<Manifest, ReadError> {
        self.read_json(&self.dir(id).join("manifest.json"))
    }

    pub fn write_manifest(&self, m: &Manifest) -> Result<(), Failure> {
        self.write_json(&self.dir(&m.id).join("manifest.json"), m)
    }

    /// The journal with every prior state read back from its payload file. A payload file that
    /// is missing is an error, never "the target did not exist": restoring that would delete a
    /// file of the user's.
    pub fn journal(&self, id: &str) -> Result<Journal, Failure> {
        self.read_journal(id).map_err(|e| e.failure)
    }

    /// `journal.json` as it is on disk, with the payload files not read yet.
    fn read_journal_file(&self, id: &str) -> Result<JournalFile, ReadError> {
        self.read_json(&self.dir(id).join(JOURNAL))
    }

    fn read_journal(&self, id: &str) -> Result<Journal, ReadError> {
        let file = self.read_journal_file(id)?;
        self.load_payloads(id, file)
    }

    /// The journal of `file` with every prior state read from its payload file.
    fn load_payloads(&self, id: &str, file: JournalFile) -> Result<Journal, ReadError> {
        let mut snapshots = Vec::with_capacity(file.snapshots.len());
        for s in file.snapshots {
            let prior = match s.payload {
                None => None,
                Some(name) if name != payload_name(s.op_index) => {
                    return Err(ReadError::new(
                        UnreadableKind::Damaged,
                        format!(
                            "{JOURNAL} of {id} names the file {name} for op {}, not {}",
                            s.op_index,
                            payload_name(s.op_index)
                        ),
                    ));
                }
                Some(_) => Some(self.read_file(&self.payload_path(id, s.op_index))?),
            };
            snapshots.push(Snapshot {
                op_index: s.op_index,
                prior,
            });
        }
        Ok(Journal {
            id: file.id,
            ops: file.ops,
            snapshots,
            stage: file.stage,
        })
    }

    /// Whether the restore point was set aside (see [`SnapshotStore::set_aside`]): it has no
    /// `journal.json`, and has either the renamed journal or the note that stands for one.
    /// `--cleanup` keeps the restore points while one is: its files may be the only copy of
    /// what it changed.
    pub fn is_set_aside(&self, id: &str) -> bool {
        let dir = self.dir(id);
        !self.fs.exists(&dir.join(JOURNAL))
            && (self.fs.exists(&dir.join(JOURNAL_ASIDE)) || self.fs.exists(&dir.join(NO_JOURNAL)))
    }

    /// Whether the restore point's transaction got as far as a journal: it has `journal.json`,
    /// the journal renamed by [`SnapshotStore::set_aside`], or an `incomplete.json`. A manifest
    /// alone, or with the note `set_aside` leaves for a journal that was never written, is a
    /// transaction that ran nothing.
    fn ran(&self, id: &str) -> bool {
        let dir = self.dir(id);
        [JOURNAL, JOURNAL_ASIDE, INCOMPLETE]
            .iter()
            .any(|name| self.fs.exists(&dir.join(name)))
    }

    /// The restore points that cannot be used, oldest first, each with why: the manifest or the
    /// journal is missing, locked or damaged, or a payload the journal names is missing or
    /// cannot be read. Restore points that were set aside are not listed again.
    ///
    /// Each entry says what kind of trouble it is, so that a caller can tell one that stays from
    /// one that may pass. A person sets aside what is [`UnreadableKind::Missing`] or
    /// [`UnreadableKind::Damaged`] (Task 13 does); what is [`UnreadableKind::CouldNotRead`] is
    /// reported and left for a later try, because the lock or the scan that kept a file from
    /// being read may be gone by then. Setting it aside would end an undo of the apply it holds.
    ///
    /// A recovery and a clean-up go on without these, so this is where they are reported: once,
    /// followed by [`SnapshotStore::set_aside`]. An undo does not go on past one: see
    /// [`SnapshotStore::latest_undoable`].
    ///
    /// A restore point listed for its manifest alone still has a journal that reads. If it is the
    /// newest and unfinished, recovery can roll it back, which setting it aside first would
    /// prevent: recover, then set aside.
    pub fn unreadable(&self) -> Result<Vec<Unreadable>, Failure> {
        let mut found = Vec::new();
        for id in self.ids()? {
            if self.is_set_aside(&id) {
                continue;
            }
            if let Err(e) = self.read_manifest(&id).and_then(|_| self.read_journal(&id)) {
                found.push(Unreadable {
                    id,
                    kind: e.kind,
                    failure: e.failure,
                });
            }
        }
        Ok(found)
    }

    /// Ends the trouble with a restore point that [`SnapshotStore::unreadable`] listed: its
    /// journal is renamed to `journal.unreadable.json`, which stays for a look and is never
    /// deleted here. From then on `unreadable` does not list it, `latest_undoable` skips it and
    /// recovery ignores it. When there is no journal to rename, a note named
    /// `journal.missing.json` takes its place. Setting aside twice is fine. The manifest and the
    /// payload files stay.
    pub fn set_aside(&self, id: &str) -> Result<(), Failure> {
        let dir = self.dir(id);
        if !self.fs.is_dir(&dir) {
            return Err(write_fail(format!("there is no restore point {id}")));
        }
        if self.is_set_aside(id) {
            return Ok(());
        }
        let journal = dir.join(JOURNAL);
        if self.fs.exists(&journal) {
            self.fs
                .rename(&journal, &dir.join(JOURNAL_ASIDE))
                .map_err(write_fail)
        } else {
            self.fs
                .write_atomic(&dir.join(NO_JOURNAL), NO_JOURNAL_NOTE)
                .map_err(write_fail)
        }
    }

    /// The newest apply that finished and was not reversed yet.
    ///
    /// It never goes past a restore point it cannot read: an apply that is newer than the answer
    /// and whose manifest or journal cannot be read could be the one to undo, and undoing the
    /// older apply instead would leave the newer one on the machine, and a lock that is gone a
    /// second later would have decided it. That is `Err(E_SNAPSHOT_READ)` naming the restore
    /// point. Only a restore point that was set aside is skipped (see
    /// [`SnapshotStore::unreadable`] and [`SnapshotStore::set_aside`]). One that is plainly not
    /// an answer is skipped as before: an undo or a revert-all, an apply that was reversed, and
    /// a transaction that did not commit.
    pub fn latest_undoable(&self) -> Result<Option<(Manifest, Journal)>, Failure> {
        let in_the_way = |id: &str, f: Failure| Failure {
            code: f.code,
            message: format!(
                "restore point {id} cannot be read, so the apply before it is not undone: {}",
                f.message
            ),
        };
        for id in self.ids()?.into_iter().rev() {
            if self.is_set_aside(&id) {
                continue;
            }
            let m = self.manifest(&id).map_err(|f| in_the_way(&id, f))?;
            if m.kind != SnapshotKind::Apply || m.undone_by.is_some() {
                continue;
            }
            let j = self.journal(&id).map_err(|f| in_the_way(&id, f))?;
            if j.stage == Stage::Committed {
                return Ok(Some((m, j)));
            }
        }
        Ok(None)
    }

    /// Marks every committed apply that is not reversed yet as reversed by `by`. Every manifest
    /// is read before the first is written, so one that cannot be read changes nothing.
    pub fn mark_undone(&self, ids: &[String], by: &str) -> Result<(), Failure> {
        let mut changed = Vec::new();
        for id in ids {
            let mut m = self.manifest(id)?;
            if m.undone_by.is_none() {
                m.undone_by = Some(by.to_string());
                changed.push(m);
            }
        }
        for m in &changed {
            self.write_manifest(m)?;
        }
        Ok(())
    }

    /// Deletes all but the first-ever and the `keep` newest restore points, and returns the
    /// deleted ids. A restore point that was set aside counts like any other: among the newest
    /// it stays, older it goes. Unless it was set aside, a restore point is never deleted when
    /// its payload files may be the only copy of a user's file:
    /// - its journal is unfinished, or cannot be read (it may be an unfinished transaction);
    /// - its rollback ended `RollbackIncomplete`, which counts as finished but left something
    ///   that could not be put back;
    /// - it has an `incomplete.json`, which says what to copy back from where.
    ///
    /// A folder that cannot be deleted does not keep the others from going; the first such
    /// failure is returned at the end.
    pub fn prune(&self, keep: usize) -> Result<Vec<String>, Failure> {
        let ids = self.ids()?;
        let cut = ids.len().saturating_sub(keep);
        let mut deleted = Vec::new();
        let mut first_failure = None;
        for id in ids.iter().skip(1).take(cut.saturating_sub(1)) {
            if !self.may_go(id) {
                continue;
            }
            match self.fs.remove_dir_all(&self.dir(id)) {
                Ok(()) => deleted.push(id.clone()),
                Err(e) => {
                    first_failure.get_or_insert_with(|| write_fail(e));
                }
            }
        }
        match first_failure {
            Some(failure) => Err(failure),
            None => Ok(deleted),
        }
    }

    /// Whether [`SnapshotStore::prune`] may delete the restore point, once it is old enough.
    fn may_go(&self, id: &str) -> bool {
        if self.is_set_aside(id) {
            return true;
        }
        if self.fs.exists(&self.dir(id).join(INCOMPLETE)) {
            return false;
        }
        self.journal(id)
            .is_ok_and(|j| j.is_finished() && !matches!(j.stage, Stage::RollbackIncomplete { .. }))
    }

    /// A journal store for a new transaction `manifest.id`.
    pub fn begin(&self, manifest: Manifest) -> DiskJournal<'a> {
        DiskJournal {
            fs: self.fs,
            root: self.root.to_path_buf(),
            id: Some(manifest.id.clone()),
            manifest: Some(manifest),
            written: BTreeSet::new(),
        }
    }

    /// A journal store for crash recovery: `load_unfinished` finds the newest
    /// restore point, and returns its journal when it is unfinished.
    pub fn recovery(&self) -> DiskJournal<'a> {
        DiskJournal {
            fs: self.fs,
            root: self.root.to_path_buf(),
            id: None,
            manifest: None,
            written: BTreeSet::new(),
        }
    }

    /// The file that holds the prior state of op `op_index` of restore point `id`.
    pub fn payload_path(&self, id: &str, op_index: usize) -> PathBuf {
        self.dir(id).join("payload").join(format!("{op_index}.bin"))
    }

    /// Writes `incomplete.json` into the restore point the record names, which must exist.
    pub fn write_incomplete(&self, record: &Incomplete) -> Result<(), Failure> {
        let dir = self.dir(&record.id);
        if !self.fs.is_dir(&dir) {
            return Err(write_fail(format!(
                "there is no restore point {}",
                record.id
            )));
        }
        self.write_json(&dir.join(INCOMPLETE), record)
    }

    /// The record of the newest transaction that ran, when it ended `RollbackIncomplete`:
    /// nothing has run since, so what it lists may still need a hand. `None` otherwise.
    ///
    /// A restore point that ran nothing (a manifest and no journal: the first write of the
    /// journal failed) is not a transaction that has run since, so it does not hide the record
    /// of an earlier one. One whose journal was set aside did run, and does.
    pub fn latest_incomplete(&self) -> Result<Option<Incomplete>, Failure> {
        let Some(id) = self.ids()?.into_iter().rev().find(|id| self.ran(id)) else {
            return Ok(None);
        };
        self.incomplete(&id)
    }

    /// The `incomplete.json` of restore point `id`, newest or not; `None` when it has none. One
    /// that cannot be read, or is damaged, is an error (`E_SNAPSHOT_READ`).
    pub fn incomplete(&self, id: &str) -> Result<Option<Incomplete>, Failure> {
        let path = self.dir(id).join(INCOMPLETE);
        match self.fs.read(&path).map_err(read_fail)? {
            None => Ok(None),
            Some(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| read_fail(format!("{} is damaged: {e}", path.display()))),
        }
    }
}

/// One op a rollback could not put back, as `incomplete.json` keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unrestored {
    pub op_index: usize,
    /// What the op touched, in plain English (`exec::op_label`).
    pub target: String,
    /// The code and the message of the restore that failed.
    pub code: String,
    pub message: String,
    /// How to put the target back by hand (`exec::fix_by_hand`), in English.
    pub fix: String,
    /// The same, in Thai (spec §5.9: what a user reads comes in both languages). A record
    /// written before this field existed has none.
    #[serde(default)]
    pub fix_th: String,
    /// The restore point whose ops `op_index` counts, when it is not the record's own: a step
    /// that a revert of everything could not put back either, carried over into the record of
    /// its own restore point. Not written while `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_point: Option<String>,
}

/// `incomplete.json`: what a transaction that ended `RollbackIncomplete` could not put back,
/// and how to do it by hand (spec §5.5). The journal keeps only the positions of those ops,
/// because a `txn::Failure` can't be read back from JSON; this record keeps the rest, next to
/// the journal, so it survives a restart. It is not in `state.json`, which is itself one of the
/// targets a rollback may have left as it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incomplete {
    /// The restore point of the transaction.
    pub id: String,
    /// The code and the reason of the failure that started the rollback.
    pub code: String,
    pub reason: String,
    pub steps: Vec<Unrestored>,
}

/// `txn::JournalStore` on disk. Payload files are written once each, before
/// the journal that names them (write-ahead, spec §5.5).
pub struct DiskJournal<'a> {
    fs: &'a dyn Fs,
    root: PathBuf,
    id: Option<String>,
    /// Written by the first `save` that gets it onto the disk.
    manifest: Option<Manifest>,
    /// Ops whose payload file is on disk.
    written: BTreeSet<usize>,
}

impl JournalStore for DiskJournal<'_> {
    /// Order: the manifest, then each payload file not yet written, then `journal.json`. Every
    /// write is atomic, so a crash leaves the journal of the save before, and never a journal
    /// that names a payload file that is not there.
    fn save(&mut self, j: &Journal) -> Result<(), Failure> {
        let id = match &self.id {
            Some(id) if *id != j.id => {
                return Err(write_fail(format!(
                    "journal {} does not belong to restore point {id}",
                    j.id
                )));
            }
            Some(id) => id.clone(),
            None => j.id.clone(),
        };
        let store = SnapshotStore {
            fs: self.fs,
            root: &self.root,
        };
        if let Some(m) = &self.manifest {
            store.write_manifest(m)?;
            // Only now: when the write fails, the next save writes the manifest again.
            self.manifest = None;
        }
        let mut refs = Vec::with_capacity(j.snapshots.len());
        for s in &j.snapshots {
            let payload = match &s.prior {
                None => None,
                Some(bytes) => {
                    if !self.written.contains(&s.op_index) {
                        self.fs
                            .write_atomic(&store.payload_path(&id, s.op_index), bytes)
                            .map_err(write_fail)?;
                        self.written.insert(s.op_index);
                    }
                    Some(payload_name(s.op_index))
                }
            };
            refs.push(PayloadRef {
                op_index: s.op_index,
                payload,
            });
        }
        let file = JournalFile {
            id: j.id.clone(),
            ops: j.ops.clone(),
            snapshots: refs,
            stage: j.stage.clone(),
        };
        store.write_json(&store.dir(&id).join(JOURNAL), &file)?;
        self.id = Some(id);
        Ok(())
    }

    /// The newest restore point's journal, when it is unfinished. A newest restore point with no
    /// `journal.json` (its first write failed, or it was set aside) has nothing to recover, nor
    /// has one whose journal says it ended: its payload files are not read. A journal that
    /// cannot be read, or an unfinished one with a payload that cannot be, is an error until the
    /// restore point is set aside.
    fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
        let store = SnapshotStore {
            fs: self.fs,
            root: &self.root,
        };
        let Some(id) = store.ids()?.pop() else {
            return Ok(None);
        };
        if !self.fs.exists(&store.dir(&id).join(JOURNAL)) {
            return Ok(None);
        }
        // The stage first: a journal that has ended needs no payload, and one that is gone or
        // locked must not make a finished transaction look like a failed recovery.
        let file = store.read_journal_file(&id).map_err(|e| e.failure)?;
        if file.is_finished() {
            return Ok(None);
        }
        let j = store.load_payloads(&id, file).map_err(|e| e.failure)?;
        self.written = j
            .snapshots
            .iter()
            .filter(|s| s.prior.is_some())
            .map(|s| s.op_index)
            .collect();
        self.id = Some(id);
        Ok(Some(j))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;
    use crate::adapters::{AdapterError, DirEntry, FsLock};

    const ROOT: &str = r"C:\d\snapshots";

    fn manifest(id: &str, kind: SnapshotKind) -> Manifest {
        Manifest {
            id: id.into(),
            kind,
            created_ms: 1,
            theme: None,
            target: Some(Target::All),
            summary: vec![],
            undoes: None,
            undone_by: None,
        }
    }

    fn journal(id: &str, stage: Stage) -> Journal {
        Journal {
            id: id.into(),
            ops: vec![
                Op::WriteFile {
                    path: PathBuf::from(r"C:\d\a.lua"),
                    contents: "new".into(),
                },
                Op::SetAutoRun { value: None },
                Op::WriteFile {
                    path: PathBuf::from(r"C:\d\empty.txt"),
                    contents: "x".into(),
                },
            ],
            snapshots: vec![
                Snapshot {
                    op_index: 0,
                    prior: Some(b"old a".to_vec()),
                },
                Snapshot {
                    op_index: 1,
                    prior: None,
                },
                Snapshot {
                    op_index: 2,
                    prior: Some(Vec::new()),
                },
            ],
            stage,
        }
    }

    #[test]
    fn journals_round_trip_through_payload_files() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let mut disk = store.begin(manifest("0000000000001-aaaaaaaa", SnapshotKind::Apply));
        let j = journal("0000000000001-aaaaaaaa", Stage::Committed);
        disk.save(&j).unwrap();
        assert_eq!(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\0.bin")),
            Some(b"old a".to_vec())
        );
        assert_eq!(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\2.bin")),
            Some(Vec::new()),
            "an empty prior is a file"
        );
        assert_eq!(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\1.bin")),
            None,
            "an absent prior has no file"
        );
        let text = String::from_utf8(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\journal.json"))
                .unwrap(),
        )
        .unwrap();
        assert!(
            text.contains("\"opIndex\": 0")
                && text.contains("\"payload\": null")
                && !text.contains("old a")
        );
        assert_eq!(store.journal("0000000000001-aaaaaaaa").unwrap(), j);
        assert_eq!(
            store.manifest("0000000000001-aaaaaaaa").unwrap().kind,
            SnapshotKind::Apply
        );
    }

    #[test]
    fn only_the_newest_unfinished_journal_is_recovered() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Applying { done: 1 }))
            .unwrap();
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Apply))
            .save(&journal("0000000000002-b", Stage::Committed))
            .unwrap();
        assert_eq!(store.recovery().load_unfinished().unwrap(), None);
        store
            .begin(manifest("0000000000003-c", SnapshotKind::Apply))
            .save(&journal("0000000000003-c", Stage::Verifying))
            .unwrap();
        let mut rec = store.recovery();
        let j = rec.load_unfinished().unwrap().unwrap();
        assert_eq!(j.id, "0000000000003-c");
        let mut done = j.clone();
        done.stage = Stage::RolledBack;
        rec.save(&done).unwrap();
        assert_eq!(
            store.journal("0000000000003-c").unwrap().stage,
            Stage::RolledBack
        );
    }

    #[test]
    fn a_missing_payload_is_an_error_not_an_absent_target() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Committed))
            .unwrap();
        fs.remove_file(Path::new(&format!(r"{ROOT}\0000000000001-a\payload\0.bin")))
            .unwrap();
        // A restore point that cannot be read is a read failure, not a write failure.
        assert_eq!(
            store.journal("0000000000001-a").unwrap_err().code,
            "E_SNAPSHOT_READ"
        );
    }

    #[test]
    fn prune_keeps_the_first_and_the_newest() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            let id = format!("{i:013}-x");
            let stage = if i == 3 {
                Stage::Applying { done: 0 }
            } else {
                Stage::Committed
            };
            store
                .begin(manifest(&id, SnapshotKind::Apply))
                .save(&journal(&id, stage))
                .unwrap();
        }
        let deleted = store.prune(KEEP).unwrap();
        // 25 ids: keep #1 and #6..#25; #2..#5 go, except unfinished #3.
        assert_eq!(
            deleted,
            vec![
                format!("{:013}-x", 2),
                format!("{:013}-x", 4),
                format!("{:013}-x", 5)
            ]
        );
        assert_eq!(store.ids().unwrap().len(), 22);
    }

    #[test]
    fn latest_undoable_skips_reversed_and_unfinished_applies() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Committed))
            .unwrap();
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Apply))
            .save(&journal("0000000000002-b", Stage::RolledBack))
            .unwrap();
        store
            .begin(manifest("0000000000003-c", SnapshotKind::Apply))
            .save(&journal("0000000000003-c", Stage::Committed))
            .unwrap();
        assert_eq!(
            store.latest_undoable().unwrap().unwrap().0.id,
            "0000000000003-c"
        );
        store
            .mark_undone(&["0000000000003-c".to_string()], "0000000000004-u")
            .unwrap();
        assert_eq!(
            store.latest_undoable().unwrap().unwrap().0.id,
            "0000000000001-a"
        );
        store
            .mark_undone(&["0000000000001-a".to_string()], "0000000000005-u")
            .unwrap();
        assert_eq!(store.latest_undoable().unwrap(), None);
    }

    #[test]
    fn ids_sort_by_time() {
        let a = new_id(5);
        let b = new_id(1_791_694_709_123);
        assert_eq!(a.len(), 22);
        assert!(a < b);
    }

    #[test]
    fn an_incomplete_rollback_is_news_until_something_newer_runs() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let failed = Stage::RollbackIncomplete { failed: vec![0] };
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", failed))
            .unwrap();
        assert_eq!(
            store.latest_incomplete().unwrap(),
            None,
            "nothing recorded yet"
        );
        let record = Incomplete {
            id: "0000000000001-a".into(),
            code: "E_DATA_WRITE".into(),
            reason: "disk full".into(),
            steps: vec![Unrestored {
                op_index: 0,
                target: r"file C:\d\a.lua".into(),
                code: "E_DATA_WRITE".into(),
                message: "denied".into(),
                fix: "Copy x over y.".into(),
                fix_th: "คัดลอก x ทับ y".into(),
                restore_point: None,
            }],
        };
        store.write_incomplete(&record).unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), Some(record));
        let text = String::from_utf8(
            fs.file(format!(r"{ROOT}\0000000000001-a\incomplete.json"))
                .unwrap(),
        )
        .unwrap();
        assert!(text.contains("\"opIndex\": 0"), "{text}");
        // the payload a fix names is the one the journal wrote
        assert_eq!(
            fs.file(store.payload_path("0000000000001-a", 0)),
            Some(b"old a".to_vec())
        );
        // a newer transaction has run since: the record is no longer the news
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Undo))
            .save(&journal("0000000000002-b", Stage::Committed))
            .unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), None);
    }

    // ---- error codes, and the order and the retries of the writes ----

    /// A restore point as an apply leaves it: manifest, journal and payload files.
    fn point(store: &SnapshotStore<'_>, id: &str, stage: Stage) {
        store
            .begin(manifest(id, SnapshotKind::Apply))
            .save(&journal(id, stage))
            .unwrap();
    }

    /// The path of a file inside restore point `id`.
    fn at(id: &str, name: &str) -> String {
        format!(r"{ROOT}\{id}\{name}")
    }

    fn record(id: &str) -> Incomplete {
        Incomplete {
            id: id.into(),
            code: "E_DATA_WRITE".into(),
            reason: "disk full".into(),
            steps: vec![],
        }
    }

    #[test]
    fn every_failed_read_of_a_restore_point_has_the_read_code() {
        // The manifest, the journal, a payload and incomplete.json, each on its own.
        type Read = fn(&SnapshotStore<'_>, &str) -> Result<(), Failure>;
        let cases: [(&str, Read); 4] = [
            ("manifest.json", |s, id| s.manifest(id).map(|_| ())),
            ("journal.json", |s, id| s.journal(id).map(|_| ())),
            (r"payload\0.bin", |s, id| s.journal(id).map(|_| ())),
            ("incomplete.json", |s, _| s.latest_incomplete().map(|_| ())),
        ];
        let id = "0000000000001-a";
        for (name, read) in cases {
            let fs = FakeFs::new();
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            point(&store, id, Stage::Committed);
            store.write_incomplete(&record(id)).unwrap();
            fs.fail_reads_of(at(id, name));
            let err = read(&store, id).unwrap_err();
            assert_eq!(err.code, "E_SNAPSHOT_READ", "{name}: {}", err.message);
            assert!(err.message.contains(name), "{name}: {}", err.message);
        }
    }

    #[test]
    fn a_damaged_or_missing_restore_point_file_is_a_read_failure() {
        let id = "0000000000001-a";
        for name in ["manifest.json", "journal.json", "incomplete.json"] {
            let fs = FakeFs::new();
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            point(&store, id, Stage::Committed);
            fs.put(at(id, name), b"{ not json");
            let err = match name {
                "manifest.json" => store.manifest(id).unwrap_err(),
                "journal.json" => store.journal(id).unwrap_err(),
                _ => store.latest_incomplete().unwrap_err(),
            };
            assert_eq!(err.code, "E_SNAPSHOT_READ", "{name}");
        }
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        assert_eq!(store.manifest("nope").unwrap_err().code, "E_SNAPSHOT_READ");
        assert_eq!(store.journal("nope").unwrap_err().code, "E_SNAPSHOT_READ");
    }

    #[test]
    fn a_failed_write_keeps_the_write_code() {
        let id = "0000000000001-a";
        for name in ["manifest.json", r"payload\0.bin", "journal.json"] {
            let fs = FakeFs::new();
            fs.fail_writes_to(at(id, name));
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            let err = store
                .begin(manifest(id, SnapshotKind::Apply))
                .save(&journal(id, Stage::Committed))
                .unwrap_err();
            assert_eq!(err.code, "E_SNAPSHOT_WRITE", "{name}: {}", err.message);
        }
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, id, Stage::Committed);
        fs.fail_writes_to(at(id, "incomplete.json"));
        assert_eq!(
            store.write_incomplete(&record(id)).unwrap_err().code,
            "E_SNAPSHOT_WRITE"
        );
        fs.fail_writes_to(at(id, "manifest.json"));
        assert_eq!(
            store
                .mark_undone(&[id.to_string()], "0000000000002-u")
                .unwrap_err()
                .code,
            "E_SNAPSHOT_WRITE"
        );
    }

    #[test]
    fn latest_undoable_never_undoes_an_older_apply_past_a_restore_point_it_cannot_read() {
        // Each of these is the newest apply, with an older apply before it. Skipping the newest
        // would undo the older one while the newest stays on the machine, and a lock that is
        // gone a second later would have made the difference. Only a restore point that was set
        // aside is skipped.
        type Break = fn(&FakeFs, &str);
        let breaks: [(&str, Break); 6] = [
            ("a journal that cannot be read", |fs, id| {
                fs.fail_reads_of(at(id, "journal.json"))
            }),
            ("no journal (its first write failed)", |fs, id| {
                fs.remove_file(Path::new(&at(id, "journal.json"))).unwrap()
            }),
            ("a payload that is gone", |fs, id| {
                fs.remove_file(Path::new(&at(id, r"payload\0.bin")))
                    .unwrap()
            }),
            ("a damaged journal", |fs, id| {
                fs.put(at(id, "journal.json"), b"{")
            }),
            ("a damaged manifest", |fs, id| {
                fs.put(at(id, "manifest.json"), b"{")
            }),
            ("a manifest that cannot be read", |fs, id| {
                fs.fail_reads_of(at(id, "manifest.json"))
            }),
        ];
        let (older, newest) = ("0000000000001-a", "0000000000002-b");
        for (what, break_it) in breaks {
            let fs = FakeFs::new();
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            point(&store, older, Stage::Committed);
            point(&store, newest, Stage::Committed);
            break_it(&fs, newest);
            let err = store.latest_undoable().unwrap_err();
            assert_eq!(err.code, "E_SNAPSHOT_READ", "{what}");
            assert!(err.message.contains(newest), "{what}: {}", err.message);
            store.set_aside(newest).unwrap();
            assert_eq!(
                store.latest_undoable().unwrap().unwrap().0.id,
                older,
                "{what}, once it is set aside"
            );
        }
    }

    #[test]
    fn a_restore_point_that_cannot_be_the_answer_does_not_stop_latest_undoable() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        // older than the answer: never looked at
        point(&store, "0000000000001-a", Stage::Committed);
        fs.fail_reads_of(at("0000000000001-a", "journal.json"));
        // the answer
        point(&store, "0000000000002-b", Stage::Committed);
        // newer, but an undo, which is never the answer
        store
            .begin(manifest("0000000000003-c", SnapshotKind::Undo))
            .save(&journal("0000000000003-c", Stage::Committed))
            .unwrap();
        fs.fail_reads_of(at("0000000000003-c", "journal.json"));
        // newer, but an apply that was undone already
        let mut undone = manifest("0000000000004-d", SnapshotKind::Apply);
        undone.undone_by = Some("0000000000003-c".into());
        store
            .begin(undone)
            .save(&journal("0000000000004-d", Stage::Committed))
            .unwrap();
        fs.fail_reads_of(at("0000000000004-d", "journal.json"));
        // newer, and a transaction that ended without applying anything
        point(&store, "0000000000005-e", Stage::RolledBack);
        assert_eq!(
            store.latest_undoable().unwrap().unwrap().0.id,
            "0000000000002-b"
        );
    }

    #[test]
    fn a_restore_point_that_ran_nothing_does_not_hide_an_incomplete_record() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let (a, b, c) = ("0000000000001-a", "0000000000002-b", "0000000000003-c");
        point(&store, a, Stage::RollbackIncomplete { failed: vec![0] });
        store.write_incomplete(&record(a)).unwrap();
        // a transaction whose first journal write failed: a manifest and nothing else
        store
            .write_manifest(&manifest(b, SnapshotKind::Apply))
            .unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), Some(record(a)));
        // told about once and set aside, it still ran nothing
        store.set_aside(b).unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), Some(record(a)));
        // something that did run
        point(&store, c, Stage::Committed);
        assert_eq!(store.latest_incomplete().unwrap(), None);
    }

    #[test]
    fn a_set_aside_journal_or_a_record_marks_a_restore_point_that_ran() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let (a, b, c) = ("0000000000001-a", "0000000000002-b", "0000000000003-c");
        point(&store, a, Stage::RollbackIncomplete { failed: vec![0] });
        store.write_incomplete(&record(a)).unwrap();
        // b had a journal that cannot be used: it ran something, and nobody knows what
        point(&store, b, Stage::Committed);
        fs.remove_file(Path::new(&at(b, r"payload\0.bin"))).unwrap();
        store.set_aside(b).unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), None);
        // c has a record but no journal: the record says it ran
        store
            .write_manifest(&manifest(c, SnapshotKind::Apply))
            .unwrap();
        store.write_incomplete(&record(c)).unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), Some(record(c)));
    }

    #[test]
    fn prune_keeps_a_restore_point_it_cannot_read_and_goes_on_past_one_it_cannot_delete() {
        // An unreadable journal may belong to an unfinished transaction, and its payloads may be
        // the only copy of a user's file: prune does not delete what it cannot show is finished.
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            point(&store, &format!("{i:013}-x"), Stage::Committed);
        }
        fs.fail_reads_of(at("0000000000002-x", "journal.json"));
        fs.remove_file(Path::new(&at("0000000000004-x", r"payload\0.bin")))
            .unwrap();
        let deleted = store.prune(KEEP).unwrap();
        assert_eq!(
            deleted,
            vec!["0000000000003-x".to_string(), "0000000000005-x".to_string()]
        );
        // a folder that cannot be deleted does not keep the others from going
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            point(&store, &format!("{i:013}-x"), Stage::Committed);
        }
        fs.fail_removes_of(format!(r"{ROOT}\0000000000003-x"));
        let err = store.prune(KEEP).unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_WRITE");
        let left = store.ids().unwrap();
        assert!(left.contains(&"0000000000003-x".to_string()));
        for gone in [2u64, 4, 5] {
            assert!(!left.contains(&format!("{gone:013}-x")), "#{gone} stayed");
        }
    }

    #[test]
    fn a_failed_manifest_write_is_tried_again_by_the_next_save() {
        // `begin` hands the manifest to the first save only until it is on disk: when that write
        // fails and the transaction saves again (to roll back), the manifest must still be written.
        let fs = FailsOnce::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let id = "0000000000001-a";
        let mut disk = store.begin(manifest(id, SnapshotKind::Apply));
        fs.arm();
        assert!(disk.save(&journal(id, Stage::Planned)).is_err());
        assert!(!fs.inner.exists(Path::new(&at(id, "manifest.json"))));
        disk.save(&journal(id, Stage::Planned)).unwrap();
        assert_eq!(store.manifest(id).unwrap().id, id);
        assert_eq!(store.journal(id).unwrap().stage, Stage::Planned);
    }

    #[test]
    fn a_journal_for_another_restore_point_is_refused() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let mut disk = store.begin(manifest("0000000000001-a", SnapshotKind::Apply));
        let err = disk
            .save(&journal("0000000000002-b", Stage::Planned))
            .unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_WRITE");
        assert_eq!(
            store.ids().unwrap(),
            Vec::<String>::new(),
            "nothing was written"
        );
    }

    #[test]
    fn a_payload_outside_the_payload_folder_is_not_read() {
        // journal.json is a file a person can edit: it may only name `payload/<op>.bin`.
        let id = "0000000000001-a";
        for evil in [
            "C:/d/outside.bin",
            "../outside.bin",
            "payload/../../outside.bin",
        ] {
            let fs = FakeFs::new();
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            point(&store, id, Stage::Committed);
            // where a reader that trusted the name would look
            fs.put(Path::new(ROOT).join(id).join(evil), b"not a prior state");
            let path = at(id, "journal.json");
            let text = String::from_utf8(fs.file(&path).unwrap()).unwrap();
            fs.put(&path, text.replace("payload/0.bin", evil));
            let err = store.journal(id).unwrap_err();
            assert_eq!(err.code, "E_SNAPSHOT_READ", "{evil}");
            assert!(err.message.contains(evil), "{}", err.message);
        }
    }

    #[test]
    fn a_record_needs_a_restore_point_and_does_not_make_one() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let err = store
            .write_incomplete(&record("0000000000001-a"))
            .unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_WRITE");
        assert_eq!(store.ids().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn mark_undone_changes_nothing_when_a_manifest_cannot_be_read() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, "0000000000001-a", Stage::Committed);
        point(&store, "0000000000002-b", Stage::Committed);
        fs.fail_reads_of(at("0000000000002-b", "manifest.json"));
        let ids = ["0000000000001-a".to_string(), "0000000000002-b".to_string()];
        let err = store.mark_undone(&ids, "0000000000003-u").unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_READ");
        assert_eq!(store.manifest("0000000000001-a").unwrap().undone_by, None);
    }

    #[test]
    fn a_failed_payload_write_leaves_no_journal_that_names_it() {
        // Payloads first, journal after: a crash between them never leaves a journal that
        // names a missing file.
        let id = "0000000000001-a";
        let fs = FakeFs::new();
        fs.fail_writes_to(at(id, r"payload\2.bin"));
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        assert!(store
            .begin(manifest(id, SnapshotKind::Apply))
            .save(&journal(id, Stage::Snapshotted))
            .is_err());
        assert!(fs.file(at(id, "journal.json")).is_none());
        assert!(fs.file(at(id, "manifest.json")).is_some());
    }

    #[test]
    fn each_payload_is_written_once() {
        let id = "0000000000001-a";
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let mut disk = store.begin(manifest(id, SnapshotKind::Apply));
        disk.save(&journal(id, Stage::Snapshotted)).unwrap();
        // from here on a payload can no longer be written: the later saves must not try
        fs.fail_writes_to(at(id, r"payload\0.bin"));
        fs.fail_writes_to(at(id, r"payload\2.bin"));
        disk.save(&journal(id, Stage::Applying { done: 1 }))
            .unwrap();
        disk.save(&journal(id, Stage::Committed)).unwrap();
        assert_eq!(store.journal(id).unwrap().stage, Stage::Committed);
    }

    #[test]
    fn a_recovered_journal_does_not_write_its_payloads_again() {
        let id = "0000000000001-a";
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, id, Stage::Verifying);
        fs.fail_writes_to(at(id, r"payload\0.bin"));
        fs.fail_writes_to(at(id, r"payload\2.bin"));
        let mut rec = store.recovery();
        let mut j = rec.load_unfinished().unwrap().unwrap();
        j.stage = Stage::RolledBack;
        rec.save(&j).unwrap();
        assert_eq!(store.journal(id).unwrap().stage, Stage::RolledBack);
    }

    #[test]
    fn only_directories_are_restore_points() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, "0000000000001-a", Stage::Committed);
        fs.put(format!(r"{ROOT}\notes.txt"), b"x");
        assert_eq!(store.ids().unwrap(), vec!["0000000000001-a".to_string()]);
    }

    // ---- restore points that cannot be read are listed, then set aside ----

    /// Restore points a person has to hear about, one of each kind, oldest first:
    /// `-b` has no journal, `-c` a journal that cannot be read, `-d` a payload that is gone and
    /// `-e` a manifest that is damaged. `-a` and `-f` (unfinished, but readable) are fine.
    fn broken_restore_points(fs: &FakeFs, store: &SnapshotStore<'_>) {
        point(store, "0000000000001-a", Stage::Committed);
        store
            .write_manifest(&manifest("0000000000002-b", SnapshotKind::Apply))
            .unwrap();
        point(store, "0000000000003-c", Stage::Committed);
        fs.fail_reads_of(at("0000000000003-c", "journal.json"));
        point(store, "0000000000004-d", Stage::Committed);
        fs.remove_file(Path::new(&at("0000000000004-d", r"payload\0.bin")))
            .unwrap();
        point(store, "0000000000005-e", Stage::Committed);
        fs.put(at("0000000000005-e", "manifest.json"), b"{");
        point(store, "0000000000006-f", Stage::Applying { done: 1 });
    }

    #[test]
    fn unreadable_lists_what_cannot_be_used_oldest_first() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        broken_restore_points(&fs, &store);
        let found = store.unreadable().unwrap();
        let ids: Vec<&str> = found.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "0000000000002-b",
                "0000000000003-c",
                "0000000000004-d",
                "0000000000005-e"
            ]
        );
        let kinds: Vec<UnreadableKind> = found.iter().map(|u| u.kind).collect();
        assert_eq!(
            kinds,
            [
                UnreadableKind::Missing,
                UnreadableKind::CouldNotRead,
                UnreadableKind::Missing,
                UnreadableKind::Damaged
            ]
        );
        let files = ["journal.json", "journal.json", "0.bin", "manifest.json"];
        for (u, file) in found.iter().zip(files) {
            assert_eq!(u.failure.code, "E_SNAPSHOT_READ", "{}", u.id);
            assert!(
                u.failure.message.contains(file),
                "{}: {}",
                u.id,
                u.failure.message
            );
        }
    }

    #[test]
    fn unreadable_says_whether_a_file_is_missing_damaged_or_could_not_be_read() {
        // Missing and damaged do not pass; a read that failed may (a lock, a scan): a person
        // sets aside the first two and leaves the third for a later try.
        use UnreadableKind::{CouldNotRead, Damaged, Missing};
        type Break = fn(&FakeFs, &str);
        let cases: [(&str, Break, UnreadableKind); 10] = [
            (
                "manifest missing",
                |fs, id| fs.remove_file(Path::new(&at(id, "manifest.json"))).unwrap(),
                Missing,
            ),
            (
                "manifest damaged",
                |fs, id| fs.put(at(id, "manifest.json"), b"{"),
                Damaged,
            ),
            (
                "manifest locked",
                |fs, id| fs.fail_reads_of(at(id, "manifest.json")),
                CouldNotRead,
            ),
            (
                "journal missing",
                |fs, id| fs.remove_file(Path::new(&at(id, "journal.json"))).unwrap(),
                Missing,
            ),
            (
                "journal damaged",
                |fs, id| fs.put(at(id, "journal.json"), b"{"),
                Damaged,
            ),
            (
                "journal of the wrong shape",
                |fs, id| fs.put(at(id, "journal.json"), b"{}"),
                Damaged,
            ),
            (
                "journal locked",
                |fs, id| fs.fail_reads_of(at(id, "journal.json")),
                CouldNotRead,
            ),
            (
                "payload gone",
                |fs, id| {
                    fs.remove_file(Path::new(&at(id, r"payload\0.bin")))
                        .unwrap()
                },
                Missing,
            ),
            (
                "payload locked",
                |fs, id| fs.fail_reads_of(at(id, r"payload\0.bin")),
                CouldNotRead,
            ),
            (
                "journal names a file it may not",
                |fs, id| {
                    let path = at(id, "journal.json");
                    let text = String::from_utf8(fs.file(&path).unwrap()).unwrap();
                    fs.put(&path, text.replace("payload/0.bin", "payload/9.bin"));
                },
                Damaged,
            ),
        ];
        for (what, break_it, kind) in cases {
            let fs = FakeFs::new();
            let store = SnapshotStore::new(&fs, Path::new(ROOT));
            point(&store, "0000000000001-a", Stage::Committed);
            break_it(&fs, "0000000000001-a");
            let found = store.unreadable().unwrap();
            assert_eq!(found.len(), 1, "{what}");
            let u = found.first().unwrap();
            assert_eq!(
                (u.id.as_str(), u.kind, u.failure.code),
                ("0000000000001-a", kind, "E_SNAPSHOT_READ"),
                "{what}: {}",
                u.failure.message
            );
        }
    }

    #[test]
    fn set_aside_keeps_the_journal_for_diagnostics_and_ends_the_trouble() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, "0000000000001-a", Stage::Committed);
        point(&store, "0000000000002-b", Stage::Committed);
        fs.remove_file(Path::new(&at("0000000000002-b", r"payload\0.bin")))
            .unwrap();
        let journal_json = fs.file(at("0000000000002-b", "journal.json")).unwrap();
        store.set_aside("0000000000002-b").unwrap();
        assert_eq!(fs.file(at("0000000000002-b", "journal.json")), None);
        assert_eq!(
            fs.file(at("0000000000002-b", "journal.unreadable.json")),
            Some(journal_json),
            "renamed, not deleted"
        );
        assert!(fs.file(at("0000000000002-b", "manifest.json")).is_some());
        assert!(fs.file(at("0000000000002-b", r"payload\2.bin")).is_some());
        assert_eq!(store.unreadable().unwrap(), vec![]);
        assert_eq!(
            store.latest_undoable().unwrap().unwrap().0.id,
            "0000000000001-a"
        );
        // and once more is no harm: two commands may find the same one
        store.set_aside("0000000000002-b").unwrap();
        assert_eq!(store.unreadable().unwrap(), vec![]);
    }

    #[test]
    fn an_unreadable_unfinished_journal_stops_recovery_until_it_is_set_aside() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, "0000000000001-a", Stage::Committed);
        point(&store, "0000000000002-b", Stage::Applying { done: 1 });
        fs.fail_reads_of(at("0000000000002-b", "journal.json"));
        let err = store.recovery().load_unfinished().unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_READ");
        store.set_aside("0000000000002-b").unwrap();
        assert_eq!(store.recovery().load_unfinished().unwrap(), None);
        // the next transaction is a restore point of its own, and recovery finds that
        point(&store, "0000000000003-c", Stage::Verifying);
        let mut rec = store.recovery();
        assert_eq!(
            rec.load_unfinished().unwrap().unwrap().id,
            "0000000000003-c"
        );
    }

    #[test]
    fn a_damaged_manifest_does_not_keep_recovery_from_a_journal_that_reads() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, "0000000000001-a", Stage::Applying { done: 1 });
        fs.put(at("0000000000001-a", "manifest.json"), b"{");
        let listed: Vec<String> = store
            .unreadable()
            .unwrap()
            .into_iter()
            .map(|u| u.id)
            .collect();
        assert_eq!(listed, ["0000000000001-a"]);
        let j = store.recovery().load_unfinished().unwrap().unwrap();
        assert_eq!(j.stage, Stage::Applying { done: 1 });
    }

    #[test]
    fn a_restore_point_without_a_journal_is_set_aside_with_a_note() {
        // The first journal write failed: nothing was changed, and nothing can be undone.
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .write_manifest(&manifest("0000000000001-a", SnapshotKind::Apply))
            .unwrap();
        assert_eq!(store.unreadable().unwrap().len(), 1);
        store.set_aside("0000000000001-a").unwrap();
        // The note has a name of its own: a renamed journal means the transaction ran, and this
        // one ran nothing.
        let note = fs
            .file(at("0000000000001-a", "journal.missing.json"))
            .unwrap();
        assert!(String::from_utf8(note).unwrap().contains("was missing"));
        assert!(!fs.exists(Path::new(&at("0000000000001-a", "journal.unreadable.json"))));
        assert!(!fs.exists(Path::new(&at("0000000000001-a", "journal.json"))));
        assert_eq!(store.unreadable().unwrap(), vec![]);
        assert_eq!(store.recovery().load_unfinished().unwrap(), None);
    }

    #[test]
    fn set_aside_fails_loudly_and_makes_no_restore_point() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let err = store.set_aside("0000000000009-z").unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_WRITE");
        assert_eq!(store.ids().unwrap(), Vec::<String>::new());
        // a journal that cannot be renamed stays where it is and is still reported
        point(&store, "0000000000001-a", Stage::Committed);
        fs.remove_file(Path::new(&at("0000000000001-a", r"payload\0.bin")))
            .unwrap();
        fs.fail_removes_of(at("0000000000001-a", "journal.json"));
        let err = store.set_aside("0000000000001-a").unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_WRITE");
        assert!(fs.exists(Path::new(&at("0000000000001-a", "journal.json"))));
        assert_eq!(store.unreadable().unwrap().len(), 1);
    }

    #[test]
    fn a_set_aside_restore_point_counts_among_the_newest_and_goes_when_old() {
        // Prune's rule for a restore point it cannot read: it stays until it is set aside
        // (a person has been told), and from then on it is pruned like any other.
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            point(&store, &format!("{i:013}-x"), Stage::Committed);
        }
        for broken in [2u64, 4, 24] {
            fs.remove_file(Path::new(&at(&format!("{broken:013}-x"), r"payload\0.bin")))
                .unwrap();
        }
        store.set_aside("0000000000002-x").unwrap();
        store.set_aside("0000000000024-x").unwrap();
        let deleted = store.prune(KEEP).unwrap();
        // #2 is old and set aside: gone. #4 is old but not set aside: stays. #24 is set aside
        // but one of the 20 newest: stays.
        let expected: Vec<String> = [2u64, 3, 5].iter().map(|i| format!("{i:013}-x")).collect();
        assert_eq!(deleted, expected);
        let left = store.ids().unwrap();
        assert!(left.contains(&"0000000000004-x".to_string()));
        assert!(left.contains(&"0000000000024-x".to_string()));
        assert_eq!(left.len(), 22);
    }

    #[test]
    fn prune_keeps_what_a_rollback_could_not_put_back_until_it_is_set_aside() {
        // The payload files of a rollback that ended RollbackIncomplete may be the only copy of
        // what it failed to put back, and incomplete.json says where. The journal counts as
        // finished, but prune must not take them.
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            let stage = if i == 2 {
                // its record could not be written (writing it is best effort)
                Stage::RollbackIncomplete { failed: vec![0] }
            } else {
                Stage::Committed
            };
            point(&store, &format!("{i:013}-x"), stage);
        }
        // a record with a journal that otherwise looks finished
        store.write_incomplete(&record("0000000000003-x")).unwrap();
        let ids =
            |nums: &[u64]| -> Vec<String> { nums.iter().map(|i| format!("{i:013}-x")).collect() };
        assert_eq!(store.prune(KEEP).unwrap(), ids(&[4, 5]));
        let left = store.ids().unwrap();
        assert!(left.contains(&ids(&[2])[0]) && left.contains(&ids(&[3])[0]));
        assert!(fs.file(store.payload_path("0000000000002-x", 0)).is_some());
        assert!(fs.file(at("0000000000003-x", "incomplete.json")).is_some());
        // a person has been told and set them aside: now they go like any other
        store.set_aside("0000000000002-x").unwrap();
        store.set_aside("0000000000003-x").unwrap();
        assert_eq!(store.prune(KEEP).unwrap(), ids(&[2, 3]));
    }

    #[test]
    fn recovery_does_not_read_the_payloads_of_a_journal_that_has_ended() {
        // A payload that is gone or locked is no reason to roll back a transaction that ended.
        let id = "0000000000001-a";
        let ended = [
            Stage::Committed,
            Stage::RolledBack,
            Stage::RollbackIncomplete { failed: vec![0] },
        ];
        for stage in ended {
            for lock in [true, false] {
                let fs = FakeFs::new();
                let store = SnapshotStore::new(&fs, Path::new(ROOT));
                point(&store, id, stage.clone());
                if lock {
                    fs.fail_reads_of(at(id, r"payload\0.bin"));
                } else {
                    fs.remove_file(Path::new(&at(id, r"payload\0.bin")))
                        .unwrap();
                }
                assert_eq!(
                    store.recovery().load_unfinished().unwrap(),
                    None,
                    "{stage:?}, payload locked: {lock}"
                );
            }
        }
        // a journal that cannot be read at all is still an error: its stage is not known
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, id, Stage::Committed);
        fs.put(at(id, "journal.json"), b"{");
        let err = store.recovery().load_unfinished().unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_READ");
    }

    #[test]
    fn recovery_of_an_unfinished_journal_needs_its_payloads() {
        let id = "0000000000001-a";
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, id, Stage::Applying { done: 1 });
        let j = store.recovery().load_unfinished().unwrap().unwrap();
        assert_eq!(j.snapshots.first().unwrap().prior, Some(b"old a".to_vec()));
        fs.fail_reads_of(at(id, r"payload\0.bin"));
        let err = store.recovery().load_unfinished().unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_READ");
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        point(&store, id, Stage::Verifying);
        fs.remove_file(Path::new(&at(id, r"payload\2.bin")))
            .unwrap();
        let err = store.recovery().load_unfinished().unwrap_err();
        assert_eq!(err.code, "E_SNAPSHOT_READ");
        assert!(err.message.contains("2.bin"), "{}", err.message);
    }

    #[test]
    fn a_later_mark_does_not_overwrite_an_earlier_one() {
        // An apply that an undo reversed stays reversed by that undo when a revert-all marks it
        // again: the manifest keeps who reversed it, and is not even written.
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let a = "0000000000001-a".to_string();
        point(&store, &a, Stage::Committed);
        store
            .mark_undone(std::slice::from_ref(&a), "0000000000002-u")
            .unwrap();
        fs.fail_writes_to(at(&a, "manifest.json"));
        store
            .mark_undone(std::slice::from_ref(&a), "0000000000003-r")
            .unwrap();
        assert_eq!(
            store.manifest(&a).unwrap().undone_by.as_deref(),
            Some("0000000000002-u")
        );
    }

    // ---- the fix by hand, in both languages ----

    #[test]
    fn an_unrestored_step_keeps_its_fix_in_thai_and_an_older_record_still_reads() {
        let step = Unrestored {
            op_index: 3,
            target: "file x".into(),
            code: "E_DATA_WRITE".into(),
            message: "denied".into(),
            fix: "Copy a over b.".into(),
            fix_th: "คัดลอก a ทับ b".into(),
            restore_point: None,
        };
        let json = serde_json::to_value(&step).unwrap();
        assert_eq!(json["fix"], "Copy a over b.");
        assert_eq!(json["fixTh"], "คัดลอก a ทับ b");
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["code", "fix", "fixTh", "message", "opIndex", "target"]
        );
        // an incomplete.json written before fixTh existed
        let old = r#"{"opIndex":0,"target":"t","code":"c","message":"m","fix":"f"}"#;
        let read: Unrestored = serde_json::from_str(old).unwrap();
        assert_eq!((read.fix.as_str(), read.fix_th.as_str()), ("f", ""));
    }

    /// `FakeFs` that can be told to fail its next `write_atomic` once. The fake's own hooks are
    /// permanent, and a retry needs a write that fails and then works.
    struct FailsOnce {
        inner: FakeFs,
        armed: std::cell::Cell<bool>,
    }

    impl FailsOnce {
        fn new() -> Self {
            FailsOnce {
                inner: FakeFs::new(),
                armed: std::cell::Cell::new(false),
            }
        }
        fn arm(&self) {
            self.armed.set(true);
        }
    }

    impl Fs for FailsOnce {
        fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
            self.inner.read(path)
        }
        fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
            if self.armed.replace(false) {
                return Err(AdapterError::new(path.display().to_string(), "failed once"));
            }
            self.inner.write_atomic(path, bytes)
        }
        fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
            self.inner.remove_file(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.inner.exists(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.inner.is_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.inner.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
            self.inner.remove_dir(path)
        }
        fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.inner.remove_dir_all(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
            self.inner.rename(from, to)
        }
        fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
            self.inner.list_dir(path)
        }
        fn try_lock(&self, path: &Path) -> Result<Option<FsLock>, AdapterError> {
            self.inner.try_lock(path)
        }
    }
}
