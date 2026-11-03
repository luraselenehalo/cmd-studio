//! WebView2's `ProcessFailed` (recovery spec §6.4): when the window's renderer exits or stops
//! answering, the page is loaded again by itself; from the third failure within a minute the
//! app gives up and asks the Yes/No question of §5.4 instead (`give_up`). Registered once the
//! window is built (`lib.rs`). Only the renderer's two kinds count: the GPU or a utility
//! process going is WebView2's own business, and the browser process going ends the window.
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Failures within this window of time count together (recovery spec §6.4: three in a minute).
pub const RENDERER_WINDOW_MS: u64 = 60_000;
/// From this many failures within [`RENDERER_WINDOW_MS`], the app stops reloading.
pub const RENDERER_FAILURES: u32 = 3;

/// The failures seen lately; `note` says whether this one is still to be reloaded (`true`) or
/// is the one to give up on (`false`). Pure, so it is tested without WebView2.
pub struct Failures {
    seen: Mutex<Vec<Instant>>,
}

impl Failures {
    pub fn new() -> Self {
        Failures {
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn note(&self, now: Instant) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.retain(|t| now.duration_since(*t) < Duration::from_millis(RENDERER_WINDOW_MS));
        seen.push(now);
        u32::try_from(seen.len()).unwrap_or(u32::MAX) < RENDERER_FAILURES
    }
}

impl Default for Failures {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers the handler on `window`'s `ICoreWebView2`. `give_up` runs on WebView2's thread (the
/// window's): it must only hand the work on (a thread that shows the box, then exits).
#[cfg(windows)]
pub fn watch_renderer(
    window: &tauri::WebviewWindow,
    give_up: impl Fn() + Send + Sync + 'static,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    };
    use webview2_com::ProcessFailedEventHandler;
    let failures = Failures::new();
    window
        .with_webview(move |webview| {
            let controller = webview.controller();
            // SAFETY: `controller` is the live controller of this window, cloned by Tauri on
            // the window's thread; the call reads one COM property into a new reference.
            let core: ICoreWebView2 = match unsafe { controller.CoreWebView2() } {
                Ok(c) => c,
                Err(_) => return,
            };
            let reloader = core.clone();
            let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                // SAFETY: `args` is the live event argument object WebView2 handed over, and
                // `kind` a stack value that outlives the call.
                unsafe { args.ProcessFailedKind(&mut kind) }?;
                if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                    && kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
                {
                    return Ok(());
                }
                if !failures.note(Instant::now()) {
                    give_up();
                    return Ok(());
                }
                // SAFETY: `reloader` is the live `ICoreWebView2` of this window.
                unsafe { reloader.Reload() }
            }));
            let mut token: i64 = 0;
            // SAFETY: `handler` is a COM object WebView2 keeps a reference to for as long as the
            // registration lives; `token` is a stack value that outlives the call.
            let _ = unsafe { core.add_ProcessFailed(&handler, &mut token) };
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two failures reload, the third within a minute gives up, and one a minute later starts
    /// the count again.
    #[test]
    fn the_third_failure_within_a_minute_gives_up() {
        let f = Failures::new();
        let t0 = Instant::now();
        assert!(f.note(t0));
        assert!(f.note(t0 + Duration::from_secs(10)));
        assert!(!f.note(t0 + Duration::from_secs(20)));
        // The two oldest are out of the window now: this one is the second again.
        assert!(f.note(t0 + Duration::from_secs(71)));
    }
}
