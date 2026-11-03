//! The Application event log through Windows' Event Log API (recovery spec §4.3): the
//! "Application Error" events 1000 (`EvtQuery` with an XPath query, `EvtNext`, `EvtRender` as
//! XML). Read only. The read runs on a thread of its own, one at a time (a second read waits for
//! the first, within its own five seconds), and is given up after five seconds in all (the wait
//! for its place, then each wait of `EvtNext`, is what is left of them): a log that is off, that
//! this user may not read, or that does not answer in time is an error, which the callers take
//! for "could not be read" and never for "no crash".
use super::{last_error, wide};
use crate::adapters::{AdapterError, AppCrash, EventLog};
use crate::eventlog::{crash_query, parse_crash, STILL_BEING_READ};
use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, ERROR_TIMEOUT,
};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};

/// How long one read of the log may take (recovery spec §4.3).
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// The most events one read takes, newest first: far more than the crashes that pause the theme.
/// The query names no program, so another program's crash loop of more than this many events
/// inside the span could push cmd's crashes out of the read (the check would then count too few,
/// never too many); a cap this high makes that unlikely.
pub const MOST_EVENTS: usize = 1000;
/// How many handles `EvtNext` gives at a time.
const BATCH: usize = 16;
/// How much further back than the caller's span the query looks, in milliseconds.
const QUERY_MARGIN_MS: u64 = 60_000;
/// How long a caller waits between its tries for the one reader's place while another read
/// holds it.
const SLOT_POLL: Duration = Duration::from_millis(50);

/// Set while a reader thread of this process runs, also one whose caller gave up waiting.
static READING: AtomicBool = AtomicBool::new(false);

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
        let query = query_since(since_ms, now_ms);
        let timeout = self.timeout;
        let events = within_time(timeout, &READING, move |deadline| {
            read_events(&query, MOST_EVENTS, deadline, timeout)
        })?;
        Ok(crashes_in(&events, since_ms, now_ms))
    }
}

/// The query for the crashes from `since_ms` on, as the caller's clock stood at `now_ms`.
///
/// `timediff` counts back from the clock of the Event Log service at the moment it runs, which
/// is later than `now_ms` by the time the call took to get there. A span of exactly
/// `now_ms - since_ms` would then leave out the events of the first moments after `since_ms`
/// (the apply), the ones that matter most. The span is [`QUERY_MARGIN_MS`] longer; the events
/// before `since_ms` that this lets in are dropped by `crashes_in`.
fn query_since(since_ms: u64, now_ms: u64) -> String {
    crash_query(
        now_ms
            .saturating_sub(since_ms)
            .saturating_add(QUERY_MARGIN_MS),
    )
}

/// The crashes among `xmls` (events as XML) that happened from `since_ms` to `now_ms`, both
/// ends in: the trait's contract. The query counts back from the clock at the moment it runs,
/// which is a little after the caller's `now_ms`. An event that is no crash report is dropped.
fn crashes_in(xmls: &[String], since_ms: u64, now_ms: u64) -> Vec<AppCrash> {
    xmls.iter()
        .filter_map(|xml| parse_crash(xml, since_ms))
        .filter(|c| (since_ms..=now_ms).contains(&c.at_ms))
        .collect()
}

/// What is left of a read's time at `now`: `None` once `deadline` has come.
fn time_left(deadline: Instant, now: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(now)
        .filter(|left| !left.is_zero())
}

/// The milliseconds of a read's time that are left at `now`, for `EvtNext`'s wait: at least 1
/// (0 would mean "do not wait"), at most what a `u32` holds. `None` once `deadline` has come.
fn ms_left(deadline: Instant, now: Instant) -> Option<u32> {
    let left = time_left(deadline, now)?;
    Some(u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1))
}

/// The error of a read that used up its time.
fn timed_out(timeout: Duration) -> AdapterError {
    AdapterError::new(
        "read the Application event log",
        format!("it did not answer within {timeout:?}"),
    )
}

/// Takes events from `next_batch` and turns each into text with `render`, until `next_batch`
/// gives an empty batch (the end of the log; `EvtNext` answering with no events is one too) or
/// `most` are taken. The whole read has until `deadline`: each call of `next_batch` is told the
/// milliseconds left, and a read whose time is used up is an error, with `timeout` named in
/// it. The events of a batch are dropped, which closes their handles, when the batch is done
/// with, also on an error.
fn drain<B, T>(
    deadline: Instant,
    timeout: Duration,
    most: usize,
    mut next_batch: impl FnMut(u32) -> Result<Vec<B>, AdapterError>,
    mut render: impl FnMut(&B) -> Result<T, AdapterError>,
) -> Result<Vec<T>, AdapterError> {
    let mut taken = Vec::new();
    while taken.len() < most {
        let wait = ms_left(deadline, Instant::now()).ok_or_else(|| timed_out(timeout))?;
        let batch = next_batch(wait)?;
        if batch.is_empty() {
            break;
        }
        for event in &batch {
            taken.push(render(event)?);
            if taken.len() >= most {
                return Ok(taken);
            }
        }
    }
    Ok(taken)
}

/// The claim on one read at a time: it is freed when dropped, also when the reader panics.
struct Slot(&'static AtomicBool);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Runs `work` on a thread of its own, with the deadline the caller has (`timeout` from now),
/// and waits until then for its answer. One that does not come in time is an error; the thread
/// is left to end by itself.
///
/// Only one reader runs at a time (`busy`). While an earlier one is still running, also one
/// whose caller gave up, a new read waits for its place, asking again every [`SLOT_POLL`],
/// within its own `timeout`; the work then runs with the time that is left, since the deadline
/// it is given is the caller's. A place that does not come free in time is the error
/// [`STILL_BEING_READ`], and no thread is started. The reader frees its place before it sends
/// its answer, so a caller's next read always finds it free.
fn within_time<T: Send + 'static>(
    timeout: Duration,
    busy: &'static AtomicBool,
    work: impl FnOnce(Instant) -> Result<T, AdapterError> + Send + 'static,
) -> Result<T, AdapterError> {
    let fail = |message: String| AdapterError::new("read the Application event log", message);
    let deadline = Instant::now() + timeout;
    let slot = loop {
        if busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break Slot(busy);
        }
        let Some(left) = time_left(deadline, Instant::now()) else {
            return Err(fail(STILL_BEING_READ.to_string()));
        };
        std::thread::sleep(left.min(SLOT_POLL));
    };
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("event-log".to_string())
        .spawn(move || {
            // Locals drop in reverse order, also when `work` panics: the place is freed first,
            // and only then does the caller hear that the reader is gone.
            let tx = tx;
            let slot = slot;
            let answer = work(deadline);
            drop(slot);
            // The receiver may have given up already; then nobody waits for the answer.
            let _ = tx.send(answer);
        })
        .map_err(|e| fail(format!("could not start the reader: {e}")))?;
    let left = time_left(deadline, Instant::now()).unwrap_or(Duration::ZERO);
    match rx.recv_timeout(left) {
        Ok(answer) => answer,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(timed_out(timeout)),
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

/// The XML of the events of the Application log that match `query`, newest first, at most
/// `most`, until `deadline`. `timeout` is the whole time a read is given, named in the error of
/// one whose time is used up.
fn read_events(
    query: &str,
    most: usize,
    deadline: Instant,
    timeout: Duration,
) -> Result<Vec<String>, AdapterError> {
    let path = wide(OsStr::new("Application"));
    let query = wide(OsStr::new(query));
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
    let next_batch = |wait_ms: u32| -> Result<Vec<Handle>, AdapterError> {
        let mut batch = [0 as EVT_HANDLE; BATCH];
        let mut returned = 0u32;
        // SAFETY: `batch` holds BATCH handles, the count given; the call writes at most that
        // many, and their number into `returned`. `wait_ms` is the time the whole read has
        // left, at least 1.
        let ok = unsafe {
            EvtNext(
                results.0,
                BATCH as u32,
                batch.as_mut_ptr(),
                wait_ms,
                0,
                &mut returned,
            )
        };
        if ok == 0 {
            // SAFETY: GetLastError only reads this thread's last error, which EvtNext set.
            let code = unsafe { GetLastError() };
            if code == ERROR_NO_MORE_ITEMS {
                return Ok(Vec::new());
            }
            // The wait given ran out: the time of the whole read, which is used up.
            if code == ERROR_TIMEOUT {
                return Err(timed_out(timeout));
            }
            return Err(last_error("EvtNext Application"));
        }
        let count = usize::try_from(returned).unwrap_or(0).min(BATCH);
        // Each handle is closed when its `Handle` goes, also when a render fails.
        Ok(batch.iter().take(count).map(|h| Handle(*h)).collect())
    };
    drain(deadline, timeout, most, next_batch, render)
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
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[test]
    fn an_answer_in_time_is_the_answer_and_a_late_one_an_error() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        // Review Focus 4: a log that does not answer is "could not be read", never "no crash".
        assert_eq!(within_time(Duration::from_secs(5), &BUSY, |_| Ok(7)), Ok(7));
        let late = within_time(Duration::from_millis(50), &BUSY, |_| {
            std::thread::sleep(Duration::from_millis(500));
            Ok(7)
        });
        let e = late.unwrap_err();
        // The time is said as it is: 50 ms is not "0 seconds".
        assert!(e.message.contains("did not answer within 50ms"), "{e:?}");
        // The late reader still holds the slot; wait for it before the next read.
        wait_until_free(&BUSY);
        let failed: Result<u8, _> = within_time(Duration::from_secs(5), &BUSY, |_| {
            Err(AdapterError::new(
                "EvtQuery Application",
                "Access is denied.",
            ))
        });
        assert_eq!(failed.unwrap_err().message, "Access is denied.");
    }

    /// Waits (two seconds at most) for the reader thread of an earlier read to end.
    fn wait_until_free(busy: &AtomicBool) {
        let started = Instant::now();
        while busy.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!busy.load(Ordering::SeqCst), "the reader did not end");
    }

    #[test]
    fn a_reader_that_panics_is_an_error_never_an_answer_and_frees_the_slot() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let crashed: Result<u8, _> = within_time(Duration::from_secs(5), &BUSY, |_| {
            panic!("the reader broke")
        });
        let e = crashed.unwrap_err();
        assert!(e.message.contains("stopped without an answer"), "{e:?}");
        // The slot is free by the time the caller hears it.
        assert!(!BUSY.load(Ordering::SeqCst));
        assert_eq!(within_time(Duration::from_secs(5), &BUSY, |_| Ok(7)), Ok(7));
    }

    #[test]
    fn a_second_caller_waits_for_the_first_and_gets_its_own_answer() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        // The start's crash check and Home's doctor read the log at the same time (m2 of the
        // final review): the one that comes second waits for the slot within its own time and
        // reads then, with the time it has left. It is never "could not be read" at once.
        let slow = within_time(Duration::from_millis(20), &BUSY, |_| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(1)
        });
        assert!(
            slow.is_err(),
            "the first caller gave up; its reader holds the slot"
        );
        let started = Instant::now();
        let second = within_time(Duration::from_secs(5), &BUSY, Ok);
        let deadline = second.unwrap();
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(200),
            "it waited for the first reader: {waited:?}"
        );
        assert!(waited < Duration::from_secs(2), "{waited:?}");
        // The work was given the caller's own deadline (five seconds from the call, which came
        // moments after `started`), not a fresh five seconds from when the place came free, 200
        // ms or more later.
        let slack = Duration::from_millis(50);
        assert!(deadline <= started + Duration::from_secs(5) + slack);
        assert!(deadline >= started + Duration::from_secs(4));
    }

    #[test]
    fn a_caller_whose_time_runs_out_while_the_slot_is_held_is_still_being_read() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let slow = within_time(Duration::from_millis(20), &BUSY, |_| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(1)
        });
        assert!(slow.is_err());
        let started = Instant::now();
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let second = within_time(Duration::from_millis(100), &BUSY, move |_| {
            flag.store(true, Ordering::SeqCst);
            Ok(2)
        });
        let e = second.unwrap_err();
        assert_eq!(e.message, STILL_BEING_READ, "{e:?}");
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(100) && waited < Duration::from_millis(350),
            "it waited its own time, not the first reader's: {waited:?}"
        );
        assert!(!ran.load(Ordering::SeqCst), "a second reader was started");
        // The first reader ends; the slot is free again.
        wait_until_free(&BUSY);
        assert_eq!(within_time(Duration::from_secs(5), &BUSY, |_| Ok(3)), Ok(3));
    }

    #[test]
    fn a_read_that_ended_leaves_the_slot_free_for_the_next_one_at_once() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        for i in 0..5000u32 {
            let answer = within_time(Duration::from_secs(5), &BUSY, move |_| Ok(i));
            assert_eq!(answer, Ok(i), "read {i}");
        }
    }

    #[test]
    fn a_reader_that_answers_with_an_error_frees_the_slot() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let failed: Result<u8, _> = within_time(Duration::from_secs(5), &BUSY, |_| {
            Err(AdapterError::new(
                "EvtNext Application",
                "Access is denied.",
            ))
        });
        assert!(failed.is_err());
        assert!(!BUSY.load(Ordering::SeqCst));
    }

    #[test]
    fn the_time_left_counts_down_and_is_never_zero_before_the_deadline() {
        let t = Instant::now();
        let deadline = t + Duration::from_secs(5);
        assert_eq!(ms_left(deadline, t), Some(5000));
        assert_eq!(ms_left(deadline, t + Duration::from_secs(2)), Some(3000));
        // Under a millisecond left is still one, never 0 (which would mean "do not wait").
        assert_eq!(
            ms_left(deadline, deadline - Duration::from_micros(400)),
            Some(1)
        );
        assert_eq!(
            ms_left(deadline, deadline - Duration::from_micros(4400)),
            Some(4)
        );
        // Used up at the deadline and after it.
        assert_eq!(ms_left(deadline, deadline), None);
        assert_eq!(ms_left(deadline, deadline + Duration::from_secs(1)), None);
        // More than a u32 can hold is the most it can.
        let far = t + Duration::from_secs(10_000_000);
        assert_eq!(ms_left(far, t), Some(u32::MAX));
    }

    /// Batches of numbers as `EvtNext` would give them; every call is noted with its wait.
    fn batches(
        mut all: Vec<Vec<u32>>,
        waits: &mut Vec<u32>,
    ) -> impl FnMut(u32) -> Result<Vec<u32>, AdapterError> + '_ {
        all.reverse();
        move |wait| {
            waits.push(wait);
            Ok(all.pop().unwrap_or_default())
        }
    }

    fn text(n: &u32) -> Result<String, AdapterError> {
        Ok(format!("e{n}"))
    }

    #[test]
    fn a_read_takes_batches_until_an_empty_one_and_keeps_their_order() {
        let (deadline, timeout) = (Instant::now() + Duration::from_secs(5), READ_TIMEOUT);
        let mut waits = Vec::new();
        let got = drain(
            deadline,
            timeout,
            100,
            batches(vec![vec![1, 2], vec![3], vec![], vec![4]], &mut waits),
            text,
        );
        // An empty batch is the end (also one `EvtNext` answered with no error), so 4 is
        // never asked for.
        assert_eq!(got.unwrap(), ["e1", "e2", "e3"]);
        assert_eq!(waits.len(), 3);
        // Each wait is what is left of the five seconds: between 1 ms and all of it.
        assert!(waits.iter().all(|w| (1..=5000).contains(w)), "{waits:?}");
    }

    #[test]
    fn a_read_stops_at_its_cap_without_asking_for_more() {
        let (deadline, timeout) = (Instant::now() + Duration::from_secs(5), READ_TIMEOUT);
        let mut waits = Vec::new();
        let got = drain(
            deadline,
            timeout,
            3,
            batches(vec![vec![1, 2], vec![3, 4], vec![5]], &mut waits),
            text,
        );
        assert_eq!(got.unwrap(), ["e1", "e2", "e3"]);
        assert_eq!(waits.len(), 2, "no third batch was asked for");
        // A cap of 0 reads nothing at all.
        let mut waits = Vec::new();
        let got = drain(
            deadline,
            timeout,
            0,
            batches(vec![vec![1]], &mut waits),
            text,
        );
        assert_eq!(got.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_read_whose_time_is_used_up_is_an_error_and_asks_for_nothing_more() {
        // Used up before the first batch: nothing is asked for.
        let mut waits = Vec::new();
        let timeout = Duration::from_millis(40);
        let got = drain(
            Instant::now() - Duration::from_millis(1),
            timeout,
            100,
            batches(vec![vec![1]], &mut waits),
            text,
        );
        let e = got.unwrap_err();
        assert!(e.message.contains("did not answer within 40ms"), "{e:?}");
        assert!(waits.is_empty());
        // Used up by a batch that took its time: that batch counts, the next is not asked for.
        let calls = std::cell::Cell::new(0);
        let got = drain(
            Instant::now() + Duration::from_millis(40),
            timeout,
            100,
            |_wait| {
                calls.set(calls.get() + 1);
                std::thread::sleep(Duration::from_millis(80));
                Ok(vec![calls.get()])
            },
            text,
        );
        assert!(got.unwrap_err().message.contains("did not answer within"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_batch_or_an_event_that_cannot_be_read_is_the_error_of_the_read() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let denied = || AdapterError::new("EvtNext Application", "Access is denied.");
        let got = drain(
            deadline,
            READ_TIMEOUT,
            100,
            |_| Err::<Vec<u32>, _>(denied()),
            text,
        );
        assert_eq!(got.unwrap_err().message, "Access is denied.");
        let renders = AtomicUsize::new(0);
        let got = drain(
            deadline,
            READ_TIMEOUT,
            100,
            |_| Ok(vec![1u32, 2, 3]),
            |n| {
                renders.fetch_add(1, Ordering::SeqCst);
                if *n == 2 {
                    Err(AdapterError::new("EvtRender", "The data is invalid."))
                } else {
                    text(n)
                }
            },
        );
        assert_eq!(got.unwrap_err().message, "The data is invalid.");
        assert_eq!(
            renders.load(Ordering::SeqCst),
            2,
            "no event after the bad one"
        );
    }

    /// A crash report of cmd.exe in Clink's DLL at `ms` (whole seconds), as the log renders it.
    fn crash_xml(ms: u64) -> String {
        format!(
            "<Event><System><TimeCreated SystemTime='{}'/></System><EventData>\
             <Data Name='AppName'>cmd.exe</Data><Data Name='ModuleName'>clink_dll_x64.dll</Data>\
             </EventData></Event>",
            crate::utc::iso(ms)
        )
    }

    #[test]
    fn one_read_takes_up_to_a_thousand_events() {
        // Another program's crash loop is far less likely to push cmd's crashes out of a read
        // that takes a thousand events. The 5 s deadline still bounds the read, and one that
        // runs out of time is an error ("could not be read"), never a short answer.
        assert_eq!(MOST_EVENTS, 1000);
    }

    #[test]
    fn the_query_looks_back_a_margin_past_the_span_so_a_late_clock_misses_no_event() {
        let (since, now) = (1_791_694_700_000u64, 1_791_694_710_000u64);
        // The span from `since` to `now`, and a margin of 60 s on top of it.
        assert_eq!(QUERY_MARGIN_MS, 60_000);
        assert_eq!(
            query_since(since, now),
            crash_query(10_000 + QUERY_MARGIN_MS)
        );
        assert!(query_since(since, now).contains("<= 70000]]"));
        // A `since` that is after `now` (a clock set back) still asks for the margin; the
        // largest span cannot overflow.
        assert_eq!(query_since(now, since), crash_query(QUERY_MARGIN_MS));
        assert_eq!(query_since(0, u64::MAX), crash_query(u64::MAX));
    }

    #[test]
    fn only_the_events_from_since_to_now_both_ends_in_are_kept() {
        let (since, now) = (1_791_694_700_000, 1_791_694_710_000);
        let xmls = vec![
            crash_xml(now + 1000),
            crash_xml(now),
            crash_xml(since + 5000),
            crash_xml(since),
            crash_xml(since - 1000),
            "<Event><System/></Event>".to_string(),
        ];
        let at: Vec<u64> = crashes_in(&xmls, since, now)
            .iter()
            .map(|c| c.at_ms)
            .collect();
        // Newest first, as the log gave them; the event after `now`, the one before `since`
        // and the one that is no crash report are gone.
        assert_eq!(at, [now, since + 5000, since]);
        // An event whose time can't be read is given the start of the span, and so is kept.
        let unreadable = crash_xml(now).replace(&crate::utc::iso(now), "not a time");
        let kept = crashes_in(&[unreadable], since, now);
        assert_eq!(kept.iter().map(|c| c.at_ms).collect::<Vec<_>>(), [since]);
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
        let started = Instant::now();
        let crashes = WinEventLog::real().app_crashes(now - CRASH_WINDOW_MS, now);
        assert!(crashes.is_ok(), "{crashes:?}");
        assert!(started.elapsed() < READ_TIMEOUT + Duration::from_secs(1));
        // A runner may have had no crash in seven days, and then the read above never renders
        // an event. The newest five events of any kind do reach EvtRender.
        let events = read_events("*", 5, Instant::now() + READ_TIMEOUT, READ_TIMEOUT).unwrap();
        assert!(
            !events.is_empty(),
            "the Application log holds no event at all"
        );
        assert!(events.len() <= 5, "{} events", events.len());
        for xml in &events {
            assert!(xml.starts_with("<Event"), "{xml}");
            assert!(xml.trim_end().ends_with("</Event>"), "{xml}");
            assert!(!xml.contains('\0'), "a NUL was kept: {xml:?}");
            // Whatever the event is, reading it as a crash report must not panic.
            let _ = parse_crash(xml, 0);
        }
    }
}
