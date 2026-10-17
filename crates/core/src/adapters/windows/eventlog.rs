//! The Application event log through Windows' Event Log API (recovery spec §4.3): the
//! "Application Error" events 1000 (`EvtQuery` with an XPath query, `EvtNext`, `EvtRender` as
//! XML). Read only. The read runs on a thread of its own and is given up after five seconds: a
//! log that is off, that this user may not read, or that does not answer in time is an error,
//! which the callers take for "could not be read" and never for "no crash".
use super::{last_error, wide};
use crate::adapters::{AdapterError, AppCrash, EventLog};
use crate::eventlog::{crash_query, parse_crash};
use std::ffi::OsStr;
use std::sync::mpsc;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS,
};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};

/// How long one read of the log may take (recovery spec §4.3).
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// The most events one read takes, newest first: far more crashes than pause the theme.
pub const MOST_EVENTS: usize = 200;
/// How many handles `EvtNext` gives at a time.
const BATCH: usize = 16;

/// This computer's Application log.
pub struct WinEventLog {
    timeout: Duration,
}

impl WinEventLog {
    /// The log, read within [`READ_TIMEOUT`].
    pub fn real() -> Self {
        WinEventLog {
            timeout: READ_TIMEOUT,
        }
    }
}

impl EventLog for WinEventLog {
    fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError> {
        let within = now_ms.saturating_sub(since_ms);
        let events = within_time(self.timeout, move || read_events(within))?;
        Ok(events
            .iter()
            .filter_map(|xml| parse_crash(xml, since_ms))
            .collect())
    }
}

/// Runs `work` on a thread of its own and waits `timeout` for its answer. One that does not
/// come in time is an error; the thread is left to end by itself.
fn within_time<T: Send + 'static>(
    timeout: Duration,
    work: impl FnOnce() -> Result<T, AdapterError> + Send + 'static,
) -> Result<T, AdapterError> {
    let fail = |message: String| AdapterError::new("read the Application event log", message);
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("event-log".to_string())
        .spawn(move || {
            // The receiver may have given up already; then nobody waits for the answer.
            let _ = tx.send(work());
        })
        .map_err(|e| fail(format!("could not start the reader: {e}")))?;
    match rx.recv_timeout(timeout) {
        Ok(answer) => answer,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(fail(format!(
            "it did not answer within {} seconds",
            timeout.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(fail("the reader stopped without an answer".to_string()))
        }
    }
}

/// An event log handle, closed when dropped.
struct Handle(EVT_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: the handle came from EvtQuery or EvtNext and is closed here, once.
            unsafe { EvtClose(self.0) };
        }
    }
}

/// The XML of the "Application Error" events 1000 of the last `within_ms` milliseconds, newest
/// first, at most [`MOST_EVENTS`].
fn read_events(within_ms: u64) -> Result<Vec<String>, AdapterError> {
    let path = wide(OsStr::new("Application"));
    let query = wide(OsStr::new(&crash_query(within_ms)));
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call; session 0 is this
    // computer.
    let results = Handle(unsafe {
        EvtQuery(
            0,
            path.as_ptr(),
            query.as_ptr(),
            EvtQueryChannelPath | EvtQueryReverseDirection,
        )
    });
    if results.0 == 0 {
        return Err(last_error("EvtQuery Application"));
    }
    let mut xmls = Vec::new();
    loop {
        let mut batch = [0 as EVT_HANDLE; BATCH];
        let mut returned = 0u32;
        // SAFETY: `batch` holds BATCH handles, the count given; the call writes at most that
        // many, and their number into `returned`. 1000 ms is the wait for each batch.
        let ok = unsafe {
            EvtNext(
                results.0,
                BATCH as u32,
                batch.as_mut_ptr(),
                1000,
                0,
                &mut returned,
            )
        };
        if ok == 0 {
            // SAFETY: GetLastError only reads this thread's last error, which EvtNext set.
            if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                break;
            }
            return Err(last_error("EvtNext Application"));
        }
        let count = usize::try_from(returned).unwrap_or(0).min(BATCH);
        // Each handle is closed when its `Handle` goes, also when a render fails.
        let events: Vec<Handle> = batch.iter().take(count).map(|h| Handle(*h)).collect();
        for event in &events {
            xmls.push(render(event)?);
            if xmls.len() >= MOST_EVENTS {
                return Ok(xmls);
            }
        }
    }
    Ok(xmls)
}

/// One event as XML (`EvtRenderEventXml`): its size first, then its text.
fn render(event: &Handle) -> Result<String, AdapterError> {
    let (mut used, mut properties) = (0u32, 0u32);
    // SAFETY: a size query: no buffer (NULL, size 0); the call writes the size it needs, in
    // bytes, into `used`, and `properties`.
    let sized = unsafe {
        EvtRender(
            0,
            event.0,
            EvtRenderEventXml,
            0,
            std::ptr::null_mut(),
            &mut used,
            &mut properties,
        )
    };
    // SAFETY: GetLastError only reads this thread's last error, which EvtRender set.
    if sized == 0 && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(last_error("EvtRender"));
    }
    let mut buffer = vec![0u16; usize::try_from(used).unwrap_or(0) / 2 + 1];
    let size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: `buffer` holds `size` writable bytes; the call writes at most that many, the
    // UTF-16 text and its closing NUL.
    let rendered = unsafe {
        EvtRender(
            0,
            event.0,
            EvtRenderEventXml,
            size,
            buffer.as_mut_ptr().cast(),
            &mut used,
            &mut properties,
        )
    };
    if rendered == 0 {
        return Err(last_error("EvtRender"));
    }
    let text: Vec<u16> = buffer.iter().copied().take_while(|u| *u != 0).collect();
    Ok(String::from_utf16_lossy(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::CRASH_WINDOW_MS;

    #[test]
    fn an_answer_in_time_is_the_answer_and_a_late_one_an_error() {
        // Review Focus 4: a log that does not answer is "could not be read", never "no crash".
        assert_eq!(within_time(Duration::from_secs(5), || Ok(7)), Ok(7));
        let late = within_time(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(500));
            Ok(7)
        });
        let e = late.unwrap_err();
        assert!(e.message.contains("did not answer within"), "{e:?}");
        let failed: Result<u8, _> = within_time(Duration::from_secs(5), || {
            Err(AdapterError::new(
                "EvtQuery Application",
                "Access is denied.",
            ))
        });
        assert_eq!(failed.unwrap_err().message, "Access is denied.");
    }

    #[test]
    fn this_computers_application_log_answers_within_its_time() {
        // Reads this computer's Application log: on CI's runner and the test VMs only
        // (CMDSTUDIO_SYSTEM_TESTS=1), never on a dev PC by default.
        if std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1") {
            eprintln!("SKIPPED the Application log read: it runs with CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap())
            .unwrap();
        let started = std::time::Instant::now();
        let crashes = WinEventLog::real().app_crashes(now - CRASH_WINDOW_MS, now);
        assert!(crashes.is_ok(), "{crashes:?}");
        assert!(started.elapsed() < READ_TIMEOUT + Duration::from_secs(1));
    }
}
