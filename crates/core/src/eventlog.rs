//! cmd's crashes in Clink as Windows records them (recovery spec §4.3): the Application log's
//! "Application Error" events 1000, read through [`EventLog`] as XML, and only their EventData
//! fields and their time. The message Windows formats for an event is in the language of the
//! machine, so it is never read.
use crate::adapters::{AdapterError, AppCrash, EventLog};
use crate::recovery::CRASH_WINDOW_MS;
use crate::utc;

/// The faulting module of a crash in Clink: `clink_dll_x64.dll`, ASCII case ignored.
const CLINK_DLL: &str = "clink_dll";
/// The EventData fields of the crashed program and of the faulting module, as Windows 10 and 11
/// name them. Spike S9 (`docs/spikes/phase3b-recovery/`) has not confirmed them on English and
/// Thai Windows yet (plan 3b-1 Task 4b); until it does, `parse_crash` falls back to the fields'
/// positions when none has a name.
const APP_FIELD: &str = "AppName";
const MODULE_FIELD: &str = "ModuleName";

/// The XPath query of the Application log for the "Application Error" events 1000 of the last
/// `within_ms` milliseconds (`timediff` counts back from now).
pub fn crash_query(within_ms: u64) -> String {
    format!(
        "*[System[Provider[@Name='Application Error'] and (EventID=1000) and \
         TimeCreated[timediff(@SystemTime) <= {within_ms}]]]"
    )
}

/// One event as `EvtRender` writes it in XML: the crashed program and the faulting module from
/// its EventData, and its time. The fields are read by name (`AppName`, `ModuleName`); an event
/// whose fields have no names gives them in their order, the program first and the module fourth
/// (the order of the names).
/// An event whose time can't be read is given `fallback_ms`: the query asked for its time span
/// already. `None` when the program or the module is missing.
pub fn parse_crash(xml: &str, fallback_ms: u64) -> Option<AppCrash> {
    let fields = data_fields(xml);
    let named = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| n.as_deref() == Some(name))
            .map(|(_, v)| v.clone())
    };
    let unnamed = fields.iter().all(|(n, _)| n.is_none());
    let nth = |i: usize| {
        unnamed
            .then(|| fields.get(i).map(|(_, v)| v.clone()))
            .flatten()
    };
    let app = named(APP_FIELD).or_else(|| nth(0))?;
    let module = named(MODULE_FIELD).or_else(|| nth(3))?;
    let at_ms = element_attribute(xml, "TimeCreated", "SystemTime")
        .and_then(|t| utc::parse(&t))
        .unwrap_or(fallback_ms);
    Some(AppCrash { at_ms, app, module })
}

/// Whether `crash` is cmd crashing in Clink's DLL: the program is `cmd.exe` and the module's
/// name starts with `clink_dll`, ASCII case ignored, a folder in front of either left out.
pub fn is_clink_crash(crash: &AppCrash) -> bool {
    let file = |name: &str| name.rsplit(['\\', '/']).next().unwrap_or(name).to_string();
    let app = file(&crash.app);
    let module = file(&crash.module);
    app.eq_ignore_ascii_case("cmd.exe")
        && module
            .get(..CLINK_DLL.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(CLINK_DLL))
}

/// cmd's crashes in Clink's DLL that Windows recorded since `since_ms`, and at most
/// [`CRASH_WINDOW_MS`] before `now_ms`, oldest first.
pub fn clink_crashes(
    log: &dyn EventLog,
    since_ms: u64,
    now_ms: u64,
) -> Result<Vec<AppCrash>, AdapterError> {
    let since = since_ms.max(now_ms.saturating_sub(CRASH_WINDOW_MS));
    let mut found: Vec<AppCrash> = log
        .app_crashes(since, now_ms)?
        .into_iter()
        .filter(|c| c.at_ms >= since && is_clink_crash(c))
        .collect();
    found.sort_by_key(|c| c.at_ms);
    Ok(found)
}

/// Each `<Data>` element of the event's EventData, in order: its `Name`, when it has one, and its
/// text with the XML entities read. An empty element (`<Data Name='x'/>`) gives an empty text.
fn data_fields(xml: &str) -> Vec<(Option<String>, String)> {
    let mut fields = Vec::new();
    let Some((_, after)) = xml.split_once("<EventData") else {
        return fields;
    };
    let mut rest = after
        .split_once("</EventData>")
        .map_or(after, |(body, _)| body);
    while let Some((_, after)) = rest.split_once("<Data") {
        let Some((head, tail)) = after.split_once('>') else {
            break;
        };
        // `<DataItem…>` and the like are other elements.
        if !(head.is_empty() || head.starts_with(char::is_whitespace) || head == "/") {
            rest = tail;
            continue;
        }
        let name = attribute(head, "Name");
        if head.ends_with('/') {
            fields.push((name, String::new()));
            rest = tail;
            continue;
        }
        let Some((value, after_value)) = tail.split_once("</Data>") else {
            break;
        };
        fields.push((name, decode(value)));
        rest = after_value;
    }
    fields
}

/// The attribute `attr` of the first `<element …>` in `xml`.
fn element_attribute(xml: &str, element: &str, attr: &str) -> Option<String> {
    let (_, after) = xml.split_once(&format!("<{element}"))?;
    let (head, _) = after.split_once('>')?;
    attribute(head, attr)
}

/// The value of the attribute `attr` in the inside of a start tag (`head`, the text after the
/// element's name), quoted with `'` (as `EvtRender` writes) or `"`, entities read.
fn attribute(head: &str, attr: &str) -> Option<String> {
    ['\'', '"'].into_iter().find_map(|quote| {
        let (_, after) = head.split_once(&format!(" {attr}={quote}"))?;
        let (value, _) = after.split_once(quote)?;
        Some(decode(value))
    })
}

/// XML text with its five named entities and its numeric ones read; an `&` that starts none of
/// them stays as it is.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('&') {
        out.push_str(before);
        let found = after.split_once(';').and_then(|(entity, tail)| {
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, tail))
        });
        match found {
            Some((c, tail)) => {
                out.push(c);
                rest = tail;
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// An event 1000 in the shape `EvtRender` gives on Windows 10 and 11: single quotes, named
    /// EventData fields.
    fn event(time: &str, app: &str, module: &str) -> String {
        format!(
            "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
             <Provider Name='Application Error' Guid='{{a0e9b465-b939-57d7-b27d-95d8e925ff57}}'/>\
             <EventID>1000</EventID><Version>0</Version><Level>2</Level><Task>100</Task>\
             <Opcode>0</Opcode><Keywords>0x8000000000000000</Keywords>\
             <TimeCreated SystemTime='{time}'/><EventRecordID>1234</EventRecordID>\
             <Correlation/><Execution ProcessID='5678' ThreadID='9012'/><Channel>Application</Channel>\
             <Computer>CMDSTUDIO-WIN11</Computer><Security UserID='S-1-5-21-1-2-3-1001'/></System>\
             <EventData><Data Name='AppName'>{app}</Data><Data Name='AppVersion'>10.0.26100.1</Data>\
             <Data Name='AppTimeStamp'>12345678</Data><Data Name='ModuleName'>{module}</Data>\
             <Data Name='ModuleVersion'>1.9.34.0</Data><Data Name='ModuleTimeStamp'>87654321</Data>\
             <Data Name='ExceptionCode'>c000001d</Data><Data Name='FaultingOffset'>0000000000001010</Data>\
             <Data Name='ProcessId'>0x162e</Data><Data Name='ProcessCreationTime'>0x1db3f0c2d7f1a2b</Data>\
             <Data Name='AppPath'>C:\\Windows\\system32\\cmd.exe</Data>\
             <Data Name='ModulePath'>C:\\Users\\tester\\AppData\\Local\\Temp\\clink\\dll_cache\\1.9.34.4e8ae4_97d16ab3\\clink_dll_x64.dll</Data>\
             <Data Name='IntegratorReportId'>0b6f4c3e-2a35-4e3b-9a4b-1f2e3d4c5b6a</Data>\
             <Data Name='PackageFullName'></Data><Data Name='PackageRelativeAppId'/></EventData></Event>"
        )
    }

    #[test]
    fn reads_the_program_the_module_and_the_time_by_name() {
        let xml = event(
            "2026-10-11T04:58:29.1234567Z",
            "cmd.exe",
            "clink_dll_x64.dll",
        );
        assert_eq!(
            parse_crash(&xml, 7),
            Some(AppCrash {
                at_ms: 1_791_694_709_123,
                app: "cmd.exe".to_string(),
                module: "clink_dll_x64.dll".to_string(),
            })
        );
        // Double quotes, entities in the text, and a time that can't be read.
        let xml = xml
            .replace('\'', "\"")
            .replace("2026-10-11T04:58:29.1234567Z", "not a time")
            .replace(">cmd.exe<", ">c&amp;md&#46;exe &lt;x&gt;<");
        let c = parse_crash(&xml, 7).unwrap();
        assert_eq!((c.at_ms, c.app.as_str()), (7, "c&md.exe <x>"));
    }

    #[test]
    fn fields_without_names_are_read_in_their_order() {
        let xml = "<Event><System><TimeCreated SystemTime='2026-10-11T04:58:29Z'/></System>\
                   <EventData><Data>cmd.exe</Data><Data>10.0</Data><Data>1</Data>\
                   <Data>CLINK_DLL_X64.DLL</Data><Data>1.9.34.0</Data></EventData></Event>";
        let c = parse_crash(xml, 0).unwrap();
        assert_eq!(
            (c.app.as_str(), c.module.as_str()),
            ("cmd.exe", "CLINK_DLL_X64.DLL")
        );
        assert!(is_clink_crash(&c));
        // Named fields without the two names are not read by position.
        let named = "<Event><EventData><Data Name='Other'>cmd.exe</Data></EventData></Event>";
        assert_eq!(parse_crash(named, 0), None);
        assert_eq!(parse_crash("<Event><System/></Event>", 0), None);
        assert_eq!(parse_crash("", 0), None);
        assert_eq!(
            parse_crash("<EventData><Data Name='AppName'>cmd.exe", 0),
            None
        );
    }

    #[test]
    fn only_cmd_crashing_in_clinks_dll_counts() {
        let crash = |app: &str, module: &str| AppCrash {
            at_ms: 0,
            app: app.to_string(),
            module: module.to_string(),
        };
        for (app, module) in [
            ("cmd.exe", "clink_dll_x64.dll"),
            ("CMD.EXE", "Clink_DLL_x64.dll"),
            (r"C:\Windows\System32\cmd.exe", r"C:\x\clink_dll_x86.dll"),
        ] {
            assert!(is_clink_crash(&crash(app, module)), "{app} {module}");
        }
        for (app, module) in [
            ("notepad.exe", "clink_dll_x64.dll"),
            ("cmd.exe", "ntdll.dll"),
            ("cmd.exe", "clink"),
            ("cmd.exe", "เสีย_clink_dll.dll"),
            ("cmd.exe.bak", "clink_dll_x64.dll"),
            ("", ""),
        ] {
            assert!(!is_clink_crash(&crash(app, module)), "{app} {module}");
        }
    }

    struct Log {
        crashes: Vec<AppCrash>,
        asked: RefCell<Vec<(u64, u64)>>,
    }

    impl EventLog for Log {
        fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError> {
            self.asked.borrow_mut().push((since_ms, now_ms));
            Ok(self.crashes.clone())
        }
    }

    #[test]
    fn counts_from_the_apply_and_never_more_than_seven_days_back() {
        let now = 1_791_694_709_000;
        let day = 24 * 60 * 60 * 1000;
        let at = |ms: u64, module: &str| AppCrash {
            at_ms: ms,
            app: "cmd.exe".to_string(),
            module: module.to_string(),
        };
        let log = Log {
            crashes: vec![
                at(now - 1000, "clink_dll_x64.dll"),
                at(now - 2 * day, "clink_dll_x64.dll"),
                at(now - 8 * day, "clink_dll_x64.dll"),
                at(now - 3000, "ntdll.dll"),
            ],
            asked: RefCell::new(Vec::new()),
        };
        let found = clink_crashes(&log, now - 3 * day, now).unwrap();
        assert_eq!(
            found.iter().map(|c| c.at_ms).collect::<Vec<_>>(),
            [now - 2 * day, now - 1000]
        );
        assert_eq!(clink_crashes(&log, now - 1500, now).unwrap().len(), 1);
        // An apply longer ago than seven days: seven days back.
        assert_eq!(clink_crashes(&log, 0, now).unwrap().len(), 2);
        assert_eq!(
            log.asked.borrow().as_slice(),
            [
                (now - 3 * day, now),
                (now - 1500, now),
                (now - 7 * day, now)
            ]
        );
    }

    #[test]
    fn the_query_asks_for_application_error_1000_since_the_time() {
        assert_eq!(
            crash_query(604_800_000),
            "*[System[Provider[@Name='Application Error'] and (EventID=1000) and \
             TimeCreated[timediff(@SystemTime) <= 604800000]]]"
        );
    }
}
