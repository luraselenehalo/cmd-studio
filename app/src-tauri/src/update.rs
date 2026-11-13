//! The updater behind a trait (spec §6.2, phase 4b). [`Updater`] is what the backend drives
//! (`backend/update.rs`): a check and a verified download. [`TauriUpdater`] is
//! tauri-plugin-updater behind it; the tests' [`FakeUpdater`] answers what a test scripts. The
//! plugin is used from Rust only, for those two steps: the window gets no `updater:` permission
//! (`capabilities/default.json`, pinned by `ipc.rs`), and the installer is started by the app
//! itself ([`start_installer`]), both at close and for "Update now": the plugin's own `install`
//! would start the app again with this process's arguments (`--recovery`, `--after-pid`).
//!
//! Read on 2026-11-06 in tauri-plugin-updater 2.13.3 (G-UPD), `src/updater.rs` unless named:
//! - `Update::download` (`:777`) fetches the bytes, then checks the minisign signature against
//!   the config's pubkey (`:837`-`:843`, `verify_signature` at `:1797`; since 2.13 it also
//!   compares a version in the signature's trusted comment with the announced one, `:1829`,
//!   raising `SignedVersionMismatch`, or `MissingSignedVersion` when `requireSignedVersion` is
//!   set) and returns the bytes as downloaded: the `-setup.exe` itself, or a zip it would only
//!   unpack in `install` (`extract`, `:1050`).
//! - `Update::install` on Windows (`install_inner`, `:938`) writes the bytes to a temp file
//!   (`extract_exe`, `:1091`), starts it through `ShellExecuteW` with the install mode's
//!   arguments (quiet: `/S`, `src/config.rs:47`), `/UPDATE`, and with a restart `/R /ARGS`
//!   followed by this process's own arguments (`updater_parameters`, `:982`-`:1010`), then
//!   `std::process::exit(0)` (`:979`). Known, not used.
//! - `Updater::check` (`:535`) answers `None` unless the comparator says the offered version is
//!   newer (`:669`-`:672`); with none set, `release.version > current`. The comparator set on the
//!   builder (`UpdaterBuilder::version_comparator`, `:248`) replaces the plugin's (`src/lib.rs:99`
//!   copies the plugin's state first). The targets are looked up before that answer
//!   (`get_urls`, `:675`), so a manifest without a Windows target fails even when it is not newer.
//! - `check` never reads the pubkey (only `download` does, `:840`), which is why
//!   [`TauriUpdater::check`] refuses an empty one first.
//! - `Update`'s fields are public: `version: String` (`:745`), also `download_url` and
//!   `signature` (`:752`, `:754`).
//! - `Error` (`src/error.rs:11`) is `#[non_exhaustive]`, so [`classify`] keeps a `_` arm. Against
//!   2.10.1 (diffed in the registry) it adds `PreviousAppNotRestored` (`:75`),
//!   `SignedVersionMismatch` (`:115`) and `MissingSignedVersion` (`:125`); [`classify`] names
//!   every variant.
use serde_json::Value;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A version `latest.json` offers that is newer than this build, with the release page it links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub notes_url: String,
}

/// Why a step of the updater did not go through. The variant is what the log says
/// ([`UpdateError::kind`]), never the text, which may hold a URL, except for an error no arm of
/// [`classify`] names: that one is silent as offline and keeps its text on the log line, so the
/// next version of this file can give it a kind (Q18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// No network, DNS, a server that did not answer or answered an error, a refused connection.
    Offline(String),
    /// `latest.json` could not be used: not JSON, no Windows target, a version that is not
    /// SemVer, or no public key to check it with.
    Manifest(String),
    /// The download's signature did not match the public key, or was signed for another version.
    Signature(String),
    /// The installer could not be started.
    Install(String),
    /// A plugin error none of the arms names.
    Other(String),
}

impl UpdateError {
    pub fn kind(&self) -> &'static str {
        match self {
            UpdateError::Offline(_) | UpdateError::Other(_) => "offline",
            UpdateError::Manifest(_) => "manifest",
            UpdateError::Signature(_) => "signature",
            UpdateError::Install(_) => "install",
        }
    }

    /// What the log line carries after `check=`/`download=`: the kind, and for an unclassified
    /// error its text without any path or URL (`<path>` from the first one on), folded onto one
    /// line and cut at 160 characters.
    pub fn log_text(&self) -> String {
        match self {
            UpdateError::Other(text) => {
                let folded: String = without_paths(&text.replace(['\r', '\n'], " "))
                    .chars()
                    .take(160)
                    .collect();
                format!("{} other={folded}", self.kind())
            }
            _ => self.kind().to_string(),
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Offline(why) => write!(f, "offline: {why}"),
            UpdateError::Manifest(why) => {
                write!(f, "the update manifest could not be used: {why}")
            }
            UpdateError::Signature(why) => write!(f, "the signature did not match: {why}"),
            UpdateError::Install(why) => write!(f, "the installer could not be started: {why}"),
            UpdateError::Other(why) => write!(f, "the updater failed: {why}"),
        }
    }
}

/// What the backend asks of the updater: whether a newer version is offered, and its bytes,
/// verified. Starting the installer is the backend's own ([`start_installer`]).
pub trait Updater {
    fn check(&self) -> Result<Option<Available>, UpdateError>;
    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError>;
}

/// How long the check of `latest.json` may take, from the connection to the last byte (review
/// Minor 3): a network that hangs ends as an offline check, one log line, `Idle`.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the download of the installer may take, from the connection to the last byte: an
/// installer of some tens of MB on a slow line, with room.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Runs `request` only when `pubkey` is not empty: without the founder's key nothing could be
/// verified, so nothing is asked of the network (the committed config holds none until Task 6).
fn with_pubkey<T>(
    pubkey: &str,
    request: impl FnOnce() -> Result<T, UpdateError>,
) -> Result<T, UpdateError> {
    if pubkey.trim().is_empty() {
        return Err(UpdateError::Manifest(
            "tauri.conf.json has no updater public key".to_string(),
        ));
    }
    request()
}

/// `text` with everything from its first path-like word on replaced by `<path>`: a word that
/// holds `\` or `/` (a Windows or UNC path, a Unix path, a URL) and all that follows it, since a
/// path may hold spaces and the user's name (review Minor 7). Text without one is as it was.
fn without_paths(text: &str) -> String {
    let Some(slash) = text.find(['\\', '/']) else {
        return text.to_string();
    };
    let before = text.get(..slash).unwrap_or("");
    let word_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let kept = text.get(..word_start).unwrap_or("").trim_end();
    if kept.is_empty() {
        "<path>".to_string()
    } else {
        format!("{kept} <path>")
    }
}

/// U5's rule, set explicitly on the plugin's builder: only a strictly higher version is an update.
pub fn is_newer(current: &semver::Version, offered: &semver::Version) -> bool {
    offered > current
}

/// tauri-plugin-updater, read from `plugins.updater` of tauri.conf.json (the pubkey, the
/// endpoint). What the last `check` found is kept for `download`: the plugin's `Update` holds the
/// download URL and the signature `latest.json` gave.
pub struct TauriUpdater {
    app: AppHandle,
    found: Mutex<Option<Update>>,
}

impl TauriUpdater {
    pub fn new(app: AppHandle) -> Self {
        TauriUpdater {
            app,
            found: Mutex::new(None),
        }
    }

    /// The public key of `plugins.updater`; empty until the founder's key is in (Task 6).
    fn pubkey(&self) -> String {
        self.app
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|u| u.get("pubkey"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    }
}

/// The plugin's error by its meaning (G-UPD (f): every variant of 2.13.3's `src/error.rs`).
/// Network, I/O and HTTP are `Offline`; the signature's decoding or check, and the version it
/// was signed for, are `Signature`; the manifest, the targets, the endpoint, the version and the
/// date are `Manifest`; the rest (the plugin's own build and install steps, which the app never
/// reaches but for the build's extract path, and any variant a later version adds) is `Other`:
/// silent as offline, its text on the log line.
pub fn classify(e: tauri_plugin_updater::Error) -> UpdateError {
    use tauri_plugin_updater::Error;
    let text = e.to_string();
    match e {
        Error::Network(_)
        | Error::Reqwest(_)
        | Error::Io(_)
        | Error::Http(_)
        | Error::InvalidHeaderValue(_)
        | Error::InvalidHeaderName(_) => UpdateError::Offline(text),
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => UpdateError::Signature(text),
        Error::EmptyEndpoints
        | Error::ReleaseNotFound
        | Error::TargetNotFound(_)
        | Error::TargetsNotFound(_)
        | Error::Serialization(_)
        | Error::Semver(_)
        | Error::UrlParse(_)
        | Error::UnsupportedOs
        | Error::UnsupportedArch
        | Error::InsecureTransportProtocol
        | Error::FormatDate
        | Error::InvalidUpdaterFormat => UpdateError::Manifest(text),
        Error::FailedToDetermineExtractPath
        | Error::TempDirNotOnSameMountPoint
        | Error::PreviousAppNotRestored(_)
        | Error::BinaryNotFoundInArchive
        | Error::TempDirNotFound
        | Error::AuthenticationFailed
        | Error::DebInstallFailed
        | Error::PackageInstallFailed
        | Error::Tauri(_) => UpdateError::Other(text),
        #[cfg(windows)]
        Error::Extract(_) => UpdateError::Other(text),
        _ => UpdateError::Other(text),
    }
}

impl Updater for TauriUpdater {
    fn check(&self) -> Result<Option<Available>, UpdateError> {
        let found = with_pubkey(&self.pubkey(), || {
            let updater = self
                .app
                .updater_builder()
                .version_comparator(|current, release| is_newer(&current, &release.version))
                .timeout(CHECK_TIMEOUT)
                .build()
                .map_err(classify)?;
            tauri::async_runtime::block_on(updater.check()).map_err(classify)
        })?;
        let Some(update) = found else {
            return Ok(None);
        };
        let notes_url = crate::open::release_url(&update.version).ok_or_else(|| {
            UpdateError::Manifest(format!(
                "the offered version is not SemVer: {}",
                update.version
            ))
        })?;
        let available = Available {
            version: update.version.clone(),
            notes_url,
        };
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = Some(update);
        Ok(Some(available))
    }

    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError> {
        // A copy, so the slot is not held for the whole download; `check` sets no timeout on the
        // `Update` it makes (plugin `src/updater.rs`, `timeout: None`), so the download's is set here.
        let mut update = self
            .found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|u| u.version == available.version)
            .cloned()
            .ok_or_else(|| {
                UpdateError::Manifest(format!("no check offered version {}", available.version))
            })?;
        update.timeout = Some(DOWNLOAD_TIMEOUT);
        tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).map_err(classify)
    }
}

/// The app's own start of the verified installer (spec §6.2, §5.10; U2, U3): silently (`/S`), as
/// an update (`/UPDATE`: the installer keeps the user's shortcuts as they are and skips a
/// WebView2 download, G-NSI-U), never waited for, with the child's stdio closed. With `relaunch`
/// (the user's "Update now") `/R /ARGS` with nothing after them: the installer's `.onInstSuccess`
/// starts `cmdstudio.exe` again with no arguments, a normal start. Without (the close): no `/R`,
/// and the app does not come back by itself.
#[cfg(windows)]
pub fn start_installer(file: &Path, relaunch: bool) -> Result<(), String> {
    use std::process::Stdio;
    let mut args = vec!["/S", "/UPDATE"];
    if relaunch {
        args.extend(["/R", "/ARGS"]);
    }
    std::process::Command::new(file)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} could not be started: {e}", file.display()))
}

#[cfg(not(windows))]
pub fn start_installer(_file: &Path, _relaunch: bool) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}

/// The check for a new version, once the window is up (`app_ready`, spec §6.2), on a thread of
/// its own: the window never waits for the network. The backend runs it once per run, and only
/// for a run that is its own. A thread that cannot start, or an updater that was not put into
/// Tauri's state, is one log line; the next start tries again.
#[cfg(windows)]
pub fn start_check(
    app: &AppHandle,
    backend: std::sync::Arc<crate::backend::Backend<crate::machine::RealMachine>>,
) {
    let Some(updater) = app
        .try_state::<std::sync::Arc<TauriUpdater>>()
        .map(|s| s.inner().clone())
    else {
        backend.note_line("update check=thread no-updater");
        return;
    };
    let for_thread = backend.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".to_string())
        .spawn(move || {
            for_thread.update_check_and_download(updater.as_ref());
        });
    if let Err(e) = spawned {
        backend.note_line(&format!("update check=thread {e}"));
    }
}

/// The tests' updater: answers what the test scripted and records what was asked, in order.
#[cfg(test)]
pub(crate) struct FakeUpdater<'a> {
    pub check: std::cell::RefCell<Result<Option<Available>, UpdateError>>,
    pub bytes: std::cell::RefCell<Result<Vec<u8>, UpdateError>>,
    calls: std::cell::RefCell<Vec<String>>,
    /// Runs inside `download`, before it answers: what happens while a download is in flight.
    during_download: Option<Box<dyn Fn() + 'a>>,
}

#[cfg(test)]
impl<'a> FakeUpdater<'a> {
    /// Offers `version` and the bytes `MZ-the-installer` (16 bytes) for it.
    pub fn offering(version: &str) -> Self {
        let notes_url = crate::open::release_url(version).unwrap_or_default();
        FakeUpdater {
            check: std::cell::RefCell::new(Ok(Some(Available {
                version: version.to_string(),
                notes_url,
            }))),
            bytes: std::cell::RefCell::new(Ok(b"MZ-the-installer".to_vec())),
            calls: std::cell::RefCell::new(Vec::new()),
            during_download: None,
        }
    }
    /// Up to date: the check offers nothing.
    pub fn none() -> Self {
        let fake = Self::offering("0.0.0");
        *fake.check.borrow_mut() = Ok(None);
        fake
    }
    /// The check fails with `e`.
    pub fn failing(e: UpdateError) -> Self {
        let fake = Self::offering("0.0.0");
        *fake.check.borrow_mut() = Err(e);
        fake
    }
    /// Runs `f` inside every `download`, before it answers.
    pub fn during_download(mut self, f: impl Fn() + 'a) -> Self {
        self.during_download = Some(Box::new(f));
        self
    }
    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

#[cfg(test)]
impl Updater for FakeUpdater<'_> {
    fn check(&self) -> Result<Option<Available>, UpdateError> {
        self.calls.borrow_mut().push("check".to_string());
        self.check.borrow().clone()
    }
    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError> {
        self.calls
            .borrow_mut()
            .push(format!("download {}", available.version));
        if let Some(during) = &self.during_download {
            during();
        }
        self.bytes.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U5's rule, set explicitly on the plugin's builder: only a strictly higher SemVer is an
    /// update; the same version, a lower one and a pre-release of the current one are not.
    #[test]
    fn only_a_higher_version_is_newer() {
        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert!(is_newer(&v("0.1.0"), &v("0.1.1")));
        assert!(is_newer(&v("0.1.0"), &v("0.2.0-rc.1")));
        assert!(is_newer(&v("0.1.0-spike.1"), &v("0.1.0-spike.2")));
        assert!(!is_newer(&v("0.1.0"), &v("0.1.0")));
        assert!(!is_newer(&v("0.1.0"), &v("0.0.9")));
        assert!(!is_newer(&v("0.1.0"), &v("0.1.0-rc.1")));
    }

    /// The log says the kind of a failure and never its text, except for an error no arm of
    /// `classify` names: silent as offline, its text folded onto the line so that the kind can be
    /// added (Q18).
    #[test]
    fn each_error_has_a_one_word_kind_and_an_unknown_one_keeps_its_text() {
        assert_eq!(UpdateError::Offline("dns".into()).kind(), "offline");
        assert_eq!(UpdateError::Manifest("x".into()).kind(), "manifest");
        assert_eq!(UpdateError::Signature("x".into()).kind(), "signature");
        assert_eq!(UpdateError::Install("x".into()).kind(), "install");
        assert_eq!(UpdateError::Other("x".into()).kind(), "offline");
        assert_eq!(UpdateError::Offline("dns".into()).log_text(), "offline");
        assert_eq!(
            UpdateError::Other("a new\nvariant: it said so".into()).log_text(),
            "offline other=a new variant: it said so"
        );
        let long = "x".repeat(400);
        assert_eq!(
            UpdateError::Other(long).log_text().len(),
            "offline other=".len() + 160
        );
        assert!(UpdateError::Offline("no route to host".into())
            .to_string()
            .contains("no route to host"));
    }

    /// The own spawn (U2, U3): the file is run silently as an update, with its stdio closed and
    /// never waited for; `/R /ARGS` (with nothing after them: a normal start) only when asked.
    /// Read as text: no installer is ever started on this PC.
    #[cfg(windows)]
    #[test]
    fn the_own_spawn_is_silent_and_relaunches_only_when_asked() {
        let source = include_str!("update.rs");
        let start = source.find("pub fn start_installer(").unwrap();
        let body = source.get(start..).unwrap().split_once("\n}\n").unwrap().0;
        assert!(
            body.contains(r#"let mut args = vec!["/S", "/UPDATE"];"#),
            "{body}"
        );
        assert!(body.contains(r#"args.extend(["/R", "/ARGS"]);"#), "{body}");
        assert!(
            body.contains("if relaunch {"),
            "the relaunch is asked for, never the default"
        );
        assert_eq!(
            body.matches("Stdio::null()").count(),
            3,
            "stdin, stdout and stderr closed"
        );
        assert!(
            body.contains(".spawn()") && !body.contains(".wait("),
            "never waited for"
        );
        let check = source
            .get(source.find("pub fn start_check(").unwrap()..)
            .unwrap()
            .split_once("\n}\n")
            .unwrap()
            .0;
        assert!(
            check.contains("try_state::<"),
            "no panic when the updater is not managed"
        );
        assert!(check.contains("update check=thread no-updater"));
    }

    /// The fake updater records what the backend asked of it, in order.
    #[test]
    fn the_fake_records_its_calls() {
        let fake = FakeUpdater::offering("0.2.0");
        let a = fake.check().unwrap().unwrap();
        assert_eq!(a.version, "0.2.0");
        assert_eq!(
            a.notes_url,
            "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"
        );
        assert_eq!(fake.download(&a).unwrap(), b"MZ-the-installer".to_vec());
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
        assert_eq!(FakeUpdater::none().check().unwrap(), None);
        assert_eq!(
            FakeUpdater::failing(UpdateError::Offline("x".into()))
                .check()
                .unwrap_err()
                .kind(),
            "offline"
        );
    }

    /// Review Minor 1: every plugin error a test can build lands on the kind its meaning says
    /// (the others need a value of reqwest, http, minisign, base64, url, zip or tauri).
    #[test]
    fn classify_gives_each_plugin_error_its_kind() {
        use tauri_plugin_updater::Error;
        let kind = |e: Error| classify(e).kind();
        assert_eq!(
            kind(Error::Network("503 Service Unavailable".into())),
            "offline"
        );
        assert_eq!(kind(Error::Io(std::io::Error::other("reset"))), "offline");
        assert_eq!(
            kind(Error::SignedVersionMismatch {
                signed: "0.1.0".into(),
                announced: "0.2.0".into()
            }),
            "signature"
        );
        assert_eq!(kind(Error::MissingSignedVersion), "signature");
        assert_eq!(kind(Error::SignatureUtf8("x".into())), "signature");
        for manifest in [
            Error::EmptyEndpoints,
            Error::ReleaseNotFound,
            Error::TargetNotFound("windows-x86_64".into()),
            Error::TargetsNotFound(vec![]),
            Error::Serialization(serde_json::from_str::<u8>("x").unwrap_err()),
            Error::Semver(semver::Version::parse("x").unwrap_err()),
            Error::UnsupportedOs,
            Error::UnsupportedArch,
            Error::InsecureTransportProtocol,
            Error::FormatDate,
            Error::InvalidUpdaterFormat,
        ] {
            let text = manifest.to_string();
            assert_eq!(kind(manifest), "manifest", "{text}");
        }
        for other in [
            Error::FailedToDetermineExtractPath,
            Error::TempDirNotOnSameMountPoint,
            Error::PreviousAppNotRestored(std::path::PathBuf::from("old")),
            Error::BinaryNotFoundInArchive,
            Error::TempDirNotFound,
            Error::AuthenticationFailed,
            Error::DebInstallFailed,
            Error::PackageInstallFailed,
        ] {
            let text = other.to_string();
            assert!(matches!(classify(other), UpdateError::Other(_)), "{text}");
        }
    }

    /// Review Minor 1: without a public key the request is never made, and the refusal is a
    /// manifest error (one `update check=manifest` line); with a key it is made. TauriUpdater's
    /// check goes through this gate before it builds the plugin's updater.
    #[test]
    fn an_empty_pubkey_makes_no_request() {
        for empty in ["", "   "] {
            let asked = std::cell::Cell::new(false);
            let refused = with_pubkey(empty, || {
                asked.set(true);
                Ok(())
            });
            assert!(!asked.get(), "{empty:?}: no request");
            assert_eq!(refused.unwrap_err().kind(), "manifest");
        }
        let asked = std::cell::Cell::new(false);
        with_pubkey("RWQ8dmbZ4CwhEQ", || {
            asked.set(true);
            Ok(())
        })
        .unwrap();
        assert!(asked.get());
        let source = include_str!("update.rs");
        let check = source
            .get(source.find("impl Updater for TauriUpdater {").unwrap()..)
            .unwrap()
            .split_once("\n}\n")
            .unwrap()
            .0;
        let gate = check.find("with_pubkey(&self.pubkey(), ||").unwrap();
        let builder = check.find(".updater_builder()").unwrap();
        assert!(gate < builder, "the gate comes before the plugin's updater");
    }

    /// Review Minor 3: the check and the download each have a timeout, so a hung network ends as
    /// an offline error (the backend's tests show the one line and `Idle`).
    #[test]
    fn the_check_and_the_download_have_timeouts() {
        assert_eq!(CHECK_TIMEOUT, Duration::from_secs(30));
        assert!(DOWNLOAD_TIMEOUT >= Duration::from_secs(5 * 60));
        let source = include_str!("update.rs");
        let tauri = source
            .get(source.find("impl Updater for TauriUpdater {").unwrap()..)
            .unwrap()
            .split_once("\n}\n")
            .unwrap()
            .0;
        assert!(tauri.contains(".timeout(CHECK_TIMEOUT)"), "{tauri}");
        assert!(
            tauri.contains("update.timeout = Some(DOWNLOAD_TIMEOUT);"),
            "{tauri}"
        );
    }

    /// Review Minor 7: the text an unclassified error puts on the log line never holds a path
    /// (paths hold the user's name) or a URL: from the first path-like word on, it is `<path>`.
    #[test]
    fn an_unclassified_error_never_puts_a_path_on_the_log_line() {
        let e = classify(tauri_plugin_updater::Error::PreviousAppNotRestored(
            std::path::PathBuf::from(r"C:\Users\tester\AppData\Local\CMD Studio.old"),
        ));
        assert_eq!(
            e.log_text(),
            "offline other=failed to install the update and to restore the previous app, which \
             was left at <path>"
        );
        assert!(!e.log_text().contains("tester"));
        for (text, line) in [
            (
                r"in use: C:\Users\Mary Ann\app.exe by pid 4",
                "offline other=in use: <path>",
            ),
            (r"\\server\share\tester\f", "offline other=<path>"),
            (
                "error sending request for url (https://example.com/a)",
                "offline other=error sending request for url <path>",
            ),
            (
                "opening /home/tester/x failed",
                "offline other=opening <path>",
            ),
            ("no path here", "offline other=no path here"),
        ] {
            assert_eq!(UpdateError::Other(text.into()).log_text(), line, "{text}");
        }
    }
}
