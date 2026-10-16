//! What the window gets when a command fails (spec §5.9): a stable code, the problem in Thai
//! and in English, and the technical detail behind a "more" button. The Thai and English
//! meaning of each code is a row of spec §5.9, held here once ([`meaning`]); a core error that
//! words itself in both languages keeps its own words. The UI shows `messageTh` or `messageEn`
//! by its language and `details` on request; it never composes an error text of its own.
use cmdstudio_core::actions::ActionError;
use cmdstudio_core::detect::DetectError;
use cmdstudio_core::model::ThemeError;
use cmdstudio_core::plan::PlanError;
use cmdstudio_core::resources::ResourceError;
use cmdstudio_core::state::StateError;
use cmdstudio_core::txn::Failure;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppError {
    /// A code of spec §5.9, or [`INTERNAL`].
    pub code: String,
    pub message_th: String,
    pub message_en: String,
    /// The technical text behind the problem: the core's English words, a path, a system error.
    pub details: String,
}

/// CMD Studio's own fault: a worker thread that did not answer, a poisoned lock, a dialog that
/// could not be shown. Not one of the core's codes; spec §5.9 has its row.
pub const INTERNAL: &str = "E_INTERNAL";

/// The rows of spec §5.9: code, Thai meaning, English meaning. A test reads the spec and checks
/// that both have the same codes and, for each code, the same Thai and English text.
const MEANINGS: &[(&str, &str, &str)] = &[
    (
        "E_WT_NOT_FOUND",
        r#"ไม่พบ Windows Terminal หรือพบแต่ยังไม่เคยเปิด จึงยังไม่มี `settings.json` (D6)"#,
        r#"Windows Terminal was not found, or was never opened, so it has no settings.json yet (D6)"#,
    ),
    (
        "E_WT_PARSE",
        r#"parse `settings.json` ของ WT ไม่ได้ หรือไฟล์เขียนคีย์ที่ CMD Studio ดูแลซ้ำ หรือซ้อนลึกเกิน 64 ชั้น (§5.4) ต้องแก้ไฟล์ด้วยมือ"#,
        r#"Windows Terminal's settings.json can't be parsed, writes a key CMD Studio owns twice, or nests deeper than 64 levels (§5.4); fix it by hand"#,
    ),
    (
        "E_WT_WRITE",
        r#"เขียน `settings.json` ของ WT ไม่ได้ หรือ (D7) โปรไฟล์ cmd ใน WT ไม่ตรงกับธีม"#,
        r#"Windows Terminal's settings.json could not be written, or (D7) its cmd profile does not match the theme"#,
    ),
    (
        "E_AUTORUN_FOREIGN_CLINK",
        r#"เจอ Clink ตัวอื่นใน AutoRun"#,
        r#"Another Clink runs from AutoRun"#,
    ),
    (
        "E_AUTORUN_WRITE",
        r#"เขียน AutoRun ไม่ได้ หรือแก้อย่างปลอดภัยไม่ได้จึงต้องแก้ด้วยมือ หรือ (D4) ส่วนของเราหายไปหรือไม่อยู่หน้าสุด"#,
        r#"AutoRun could not be written, or can't be changed safely and needs a hand edit, or (D4) CMD Studio's part is gone or not first"#,
    ),
    (
        "E_CLINK_PATH",
        r#"ไม่มี path ที่ cmd ใช้เรียก Clink ได้อย่างปลอดภัย: ไม่มีทางไหนใน `<clink-launch>` ที่เป็น ASCII ล้วน (G19) หรือ path มี `%`, `"` หรือ `;` หรือไม่ใช่ Unicode ที่ถูกต้อง (§5.4) · หรือ (D3) Clink inject จากโฟลเดอร์ของมันไม่ได้ (G19)"#,
        r#"No path cmd can use to start Clink safely: no `<clink-launch>` choice is all ASCII (G19), or a path holds `%`, `"` or `;` or is not valid Unicode (§5.4); or (D3) Clink can't inject from its folder (G19)"#,
    ),
    (
        "E_FONT_INSTALL",
        r#"ติดตั้งฟอนต์ไม่สำเร็จ หรือ (D8) ไม่พบฟอนต์ของธีม"#,
        r#"The font could not be installed, or (D8) the theme's font is not installed"#,
    ),
    (
        "E_RUNTIME_MISSING",
        r#"ไฟล์ runtime หาย ถูกแก้ หรือรันไม่ได้ (D1–D4) หรือไฟล์ที่แนบมากับ CMD Studio หายหรือเสีย"#,
        r#"Runtime files are missing, changed or do not run (D1–D4), or the files that come with CMD Studio are missing or damaged"#,
    ),
    (
        "E_VERIFY_OMP",
        r#"ขั้นตรวจผล: OMP เรนเดอร์ config ไม่ได้"#,
        r#"Verify: Oh My Posh can't render the config"#,
    ),
    (
        "E_VERIFY_CLINK",
        r#"ขั้นตรวจผล: Clink โหลดสคริปต์ไม่ได้ (D3 ด้วย)"#,
        r#"Verify: Clink can't load the scripts (D3 too)"#,
    ),
    (
        "E_VERIFY_READBACK",
        r#"ขั้นตรวจผล: ค่าที่อ่านกลับมาไม่ตรงกับที่เขียน"#,
        r#"Verify: a value read back differs from what was written"#,
    ),
    (
        "E_SNAPSHOT_WRITE",
        r#"ถ่ายจุดย้อนกลับไม่สำเร็จ"#,
        r#"A restore point could not be written"#,
    ),
    (
        "E_THEME_INVALID",
        r#"ไฟล์ธีมผิดรูปแบบ"#,
        r#"The theme file is not valid"#,
    ),
    (
        "E_THEME_NEWER_SCHEMA",
        r#"ไฟล์ธีมมาจากโปรแกรมรุ่นใหม่กว่า"#,
        r#"The theme file comes from a newer version"#,
    ),
    (
        "E_ROLLBACK_INCOMPLETE",
        r#"ย้อนกลับได้ไม่ครบ"#,
        r#"The rollback did not finish; some parts must be fixed by hand"#,
    ),
    (
        "E_TXN_INTERRUPTED",
        r#"การเปลี่ยนแปลงครั้งก่อนถูกตัดกลางทาง (โปรแกรมดับ) และย้อนกลับให้ตอนเริ่มคำสั่งถัดไปที่เปลี่ยนเครื่อง · doctor แจ้งเมื่อยังไม่ได้ย้อน (§5.5)"#,
        r#"An earlier change was cut off (the app stopped); it is rolled back when the next command that changes the machine starts, and doctor reports one not rolled back yet (§5.5)"#,
    ),
    (
        "E_UNSUPPORTED_PLATFORM",
        r#"เครื่องนี้ไม่ใช่ Windows แบบ x64 (เช่น ARM64) ส่วนเสริมที่แนบมากับ v1 มีแต่รุ่น x64"#,
        r#"This machine is not x64 Windows (for example ARM64); the add-ons bundled with v1 are x64 only"#,
    ),
    (
        "E_DATA_WRITE",
        r#"เขียนลงโฟลเดอร์ข้อมูลของโปรแกรมไม่ได้ เช่น ไฟล์ใน `generated\` หรือ `state.json`"#,
        r#"Could not write to CMD Studio's data folder, such as a file in `generated\` or state.json"#,
    ),
    (
        "E_STATE_INVALID",
        r#"ใช้ `state.json` ไม่ได้ เพราะไฟล์เสีย ถูกแก้ เปิดไม่ได้ หรือเขียนโดย CMD Studio รุ่นที่ใหม่กว่า · ไฟล์ที่เปิดไม่ได้หรือมาจากรุ่นใหม่กว่าจะไม่ถูกเขียนทับ ส่วนไฟล์ที่เสีย "ย้อนกลับ" จะเอา `state.json` ก่อนใช้ธีมครั้งนั้นมาแทน และ "ย้อนทุกอย่าง" กับ `--cleanup` จะเปลี่ยนชื่อเป็น `state.json.damaged-<ms>` เก็บไว้ให้ดู"#,
        r#"state.json can't be used: it is damaged, was edited, could not be opened, or was written by a newer CMD Studio. One that could not be opened or is from a newer CMD Studio is never written over; a damaged one is replaced by undo with the state.json from before that apply, and renamed state.json.damaged-<ms> by Revert everything and --cleanup"#,
    ),
    (
        "E_GENERATED_EDITED",
        r#"มีคนแก้ไฟล์ใน `generated\` ด้วยมือ (D9)"#,
        r#"A file in `generated\` was edited by hand (D9)"#,
    ),
    (
        "E_APP_PACKAGED",
        r#"แอปรันอยู่ในแพ็กเกจ MSIX สิ่งที่แอปเขียนจึงไม่มีผลกับโปรแกรมอื่น (D10, §2.2 ข้อ 1)"#,
        r#"The app runs inside an MSIX package, so what it writes is invisible to other programs (D10, §2.2 lesson 1)"#,
    ),
    (
        "E_THEME_NOT_LOADED",
        r#"ใช้ธีมแล้วแต่ยังไม่มีหน้าต่าง cmd ไหนโหลดธีม ให้เปิดหน้าต่างใหม่ ในโหมด "cmd ทุกหน้าต่าง" ถ้าเปิดแล้วยังเป็นแบบนี้ แปลว่า cmd หยุดก่อนถึงส่วนของเราใน AutoRun ต้องแก้ค่า AutoRun ด้วยมือ ส่วนโหมด "เฉพาะใน Windows Terminal" ให้เปิดแท็บ cmd ใหม่ใน WT และถ้ายังเป็นแบบนี้ให้ใช้ธีมซ้ำ (D3)"#,
        r#"The theme is applied, but no cmd window has loaded it yet: open a new one. For every cmd window, if a new window does not load it either, cmd stops before CMD Studio's part of AutoRun, and AutoRun needs a hand edit; for Windows Terminal only, open a new cmd tab there, and if that does not load it either, apply the theme again (D3)"#,
    ),
    (
        "E_MACHINE_READ",
        r#"อ่านการตั้งค่าของเครื่องนี้ไม่ได้ (registry, ไฟล์ หรือรายชื่อฟอนต์) จึงยังไม่ได้เปลี่ยนการตั้งค่าของคุณ หรือ doctor ตรวจข้อนั้นไม่ได้"#,
        r#"Could not read this computer's settings (the registry, a file, the font list), so your settings were not changed, or doctor could not check that item"#,
    ),
    (
        "E_SNAPSHOT_READ",
        r#"อ่านจุดย้อนกลับไม่ได้ (ไฟล์หาย เสีย หรือเปิดไม่ได้ในตอนนี้) · ถ้าเป็นจุดของการเปลี่ยนแปลงที่ถูกตัดกลางทาง แปลว่ายังตรวจการเปลี่ยนแปลงครั้งก่อนไม่ได้ จึงยังไม่ได้เปลี่ยนการตั้งค่าของคุณ ให้ลองใหม่ (§5.5)"#,
        r#"A restore point can't be read (missing, damaged, or not readable right now). When it is the one of a change that was cut off, an earlier change could not be checked yet: your settings were not changed; try again (§5.5)"#,
    ),
    (
        "E_BUSY",
        r#"มี CMD Studio อีกตัว หรือโปรแกรมอื่น เปิดไฟล์ `cmdstudio.lock` อยู่ คือกำลังเปลี่ยนการตั้งค่าของเครื่องนี้ ให้ลองใหม่เมื่อทำเสร็จ (§5.5)"#,
        r#"Another CMD Studio, or another program, has cmdstudio.lock open: it is changing this computer right now. Try again when it is done (§5.5)"#,
    ),
    (
        "E_INTERNAL",
        r#"CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย"#,
        r#"CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics"#,
    ),
];

/// The Thai and English meaning of a code of spec §5.9.
pub fn meaning(code: &str) -> Option<(&'static str, &'static str)> {
    MEANINGS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, th, en)| (*th, *en))
}

impl AppError {
    /// An error whose texts are the §5.9 meaning of `code`, with `details` behind them. The
    /// codes are the rows of §5.9 and no other: a code that is not a row becomes [`INTERNAL`],
    /// with that row's texts and the code it was given in front of the details, so nothing is
    /// lost.
    pub fn coded(code: &str, details: impl Into<String>) -> Self {
        let details = details.into();
        let (code, details) = if meaning(code).is_some() {
            (code, details)
        } else {
            (INTERNAL, format!("{code}: {details}"))
        };
        // E_INTERNAL is a row (a test compares the table with the spec); the details stand in
        // for the texts only if that ever failed.
        let (th, en) = meaning(code)
            .map(|(th, en)| (th.to_string(), en.to_string()))
            .unwrap_or_else(|| (details.clone(), details.clone()));
        AppError {
            code: code.to_string(),
            message_th: th,
            message_en: en,
            details,
        }
    }

    /// [`INTERNAL`] with `details`.
    pub fn internal(details: impl Into<String>) -> Self {
        AppError::coded(INTERNAL, details)
    }

    /// The same error with the Thai text the core worded itself.
    fn with_th(mut self, th: Option<String>) -> Self {
        if let Some(th) = th {
            self.message_th = th;
        }
        self
    }
}

impl From<ActionError> for AppError {
    /// The core's English text is the detail; its own Thai, where it has one (`E_BUSY`,
    /// `E_UNSUPPORTED_PLATFORM`, a pending record, a pending recovery, the lock), is the Thai
    /// message, and the English text is then the English message too, because it says more than
    /// the code's row.
    fn from(e: ActionError) -> Self {
        let text = e.to_string();
        let th = e.message_th();
        let mut out = AppError::coded(e.code(), text.clone());
        if th.is_some() {
            out.message_en = text;
        }
        out.with_th(th)
    }
}

impl From<ThemeError> for AppError {
    fn from(e: ThemeError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<DetectError> for AppError {
    fn from(e: DetectError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<StateError> for AppError {
    fn from(e: StateError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<PlanError> for AppError {
    fn from(e: PlanError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<ResourceError> for AppError {
    fn from(e: ResourceError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<Failure> for AppError {
    fn from(f: Failure) -> Self {
        AppError::coded(f.code, f.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    /// The spec, found from this crate upwards: `docs/superpowers/specs/…` in the nearest ancestor
    /// that has it (the repository root).
    fn spec_text() -> String {
        let rel = Path::new("docs/superpowers/specs/2026-10-10-cmd-studio-design.md");
        let found = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|dir| dir.join(rel))
            .find(|path| path.exists())
            .unwrap();
        std::fs::read_to_string(found).unwrap()
    }

    /// The table of spec §5.9 by code: the Thai and the English meaning. Its rows start with
    /// "| `E_" and have three cells, "| `E_CODE` | Thai | English |".
    fn spec_rows() -> BTreeMap<String, (String, String)> {
        let spec = spec_text();
        let mut rows = BTreeMap::new();
        for line in spec.lines() {
            let Some(rest) = line.strip_prefix("| `E_") else {
                continue;
            };
            let (name, cells) = rest.split_once("` | ").unwrap();
            let (th, en) = cells.strip_suffix(" |").unwrap().split_once(" | ").unwrap();
            assert!(!en.contains(" | "), "E_{name}: more than three cells");
            let old = rows.insert(format!("E_{name}"), (th.to_string(), en.to_string()));
            assert!(old.is_none(), "E_{name}: two rows");
        }
        rows
    }

    #[test]
    fn the_meanings_are_exactly_the_rows_of_spec_5_9() {
        let spec = spec_rows();
        let here: BTreeMap<String, (String, String)> = MEANINGS
            .iter()
            .map(|(c, th, en)| (c.to_string(), (th.to_string(), en.to_string())))
            .collect();
        assert_eq!(here.len(), MEANINGS.len(), "a code twice");
        // The same codes, and for each the same Thai and English text as the spec's row.
        assert_eq!(
            here.keys().collect::<Vec<_>>(),
            spec.keys().collect::<Vec<_>>(),
            "the codes of MEANINGS and of the rows of spec §5.9"
        );
        for (code, (th, en)) in &here {
            let (spec_th, spec_en) = &spec[code];
            assert_eq!(th, spec_th, "{code}: the Thai text differs from spec §5.9");
            assert_eq!(
                en, spec_en,
                "{code}: the English text differs from spec §5.9"
            );
            assert!(
                th.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "{code}: Thai"
            );
            assert!(
                !en.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "{code}: English"
            );
        }
    }

    #[test]
    fn a_core_error_keeps_its_code_and_its_own_words() {
        let e = AppError::from(ActionError::Busy);
        assert_eq!(e.code, "E_BUSY");
        assert_eq!(e.message_th, cmdstudio_core::actions::BUSY_TH);
        assert_eq!(e.message_en, cmdstudio_core::actions::BUSY_EN);
        assert_eq!(e.details, cmdstudio_core::actions::BUSY_EN);
        // One passed on from another module: the row's texts, the core's English as detail.
        let e = AppError::from(ThemeError::NewerSchema(7));
        assert_eq!(e.code, "E_THEME_NEWER_SCHEMA");
        assert_eq!(e.message_th, "ไฟล์ธีมมาจากโปรแกรมรุ่นใหม่กว่า");
        assert_eq!(e.message_en, "The theme file comes from a newer version");
        assert!(e.details.contains("schemaVersion 7"), "{}", e.details);
        // The codes are the rows of spec §5.9 and no other: a code that is not a row becomes
        // E_INTERNAL, with the E_INTERNAL row's texts and the code it was given in the details.
        let e = AppError::coded("E_NOT_A_ROW", "what happened");
        assert_eq!(e.code, INTERNAL);
        assert_eq!(e.details, "E_NOT_A_ROW: what happened");
        let (th, en) = meaning(INTERNAL).unwrap();
        assert_eq!((e.message_th.as_str(), e.message_en.as_str()), (th, en));
        // E_INTERNAL itself is a row: its details stay as given.
        let e = AppError::internal("x");
        assert_eq!((e.code.as_str(), e.details.as_str()), (INTERNAL, "x"));
        assert_eq!((e.message_th.as_str(), e.message_en.as_str()), (th, en));
    }

    #[test]
    fn each_core_error_passes_on_its_own_code() {
        use cmdstudio_core::adapters::AdapterError;

        /// The error converted keeps the code the core error itself gives, and with it the
        /// texts of that row of §5.9.
        macro_rules! keeps_its_code {
            ($($core:expr),+ $(,)?) => {$({
                let core = $core;
                let code = core.code();
                let e = AppError::from(core);
                assert_eq!(e.code, code);
                assert_eq!(Some((e.message_th.as_str(), e.message_en.as_str())), meaning(code));
            })+};
        }
        let io = || AdapterError::new("registry", "access denied");
        keeps_its_code!(
            DetectError::Read(io()),
            DetectError::Write(io()),
            StateError::Invalid("not JSON".to_string()),
            StateError::Newer { found: 9 },
            StateError::Io("disk full".to_string()),
            PlanError::ForeignClink {
                existing: vec!["clink inject".to_string()]
            },
            PlanError::Generate("bad".to_string()),
            ResourceError::Missing("omp".to_string()),
        );

        // A `Failure` carries its code as a field, and its text is the details.
        let f = Failure {
            code: "E_SNAPSHOT_WRITE",
            message: "disk full".to_string(),
        };
        let e = AppError::from(f.clone());
        assert_eq!((e.code.as_str(), e.details.as_str()), (f.code, "disk full"));
        // The same failure inside an ActionError keeps its code.
        let e = AppError::from(ActionError::Snapshot(Failure {
            code: "E_SNAPSHOT_READ",
            message: "gone".to_string(),
        }));
        assert_eq!(
            (e.code.as_str(), e.details.as_str()),
            ("E_SNAPSHOT_READ", "gone")
        );
    }

    #[test]
    fn the_error_is_camel_case_json() {
        let e = AppError::coded("E_BUSY", "d");
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        // Sorted, so the test does not depend on the map order of serde_json (`preserve_order`).
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "details", "messageEn", "messageTh"]);
    }
}
