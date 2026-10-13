//! The runtime files CMD Studio ships (spec §5.1 `resources/`, §6.2): Oh My
//! Posh, Clink (x64) and the Nerd Font, with the SHA-256 of every file in
//! `manifest.json`, written by `scripts/fetch-resources.ps1`.
use crate::adapters::Fs;
use crate::plan::{Component, CLINK_VERSION, NERD_FONT_FAMILY, OMP_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "manifest.json";
/// The bundled Nerd Font file; its GDI family is `CaskaydiaCove NF` (G12).
pub const NERD_FONT_FILE: &str = "CaskaydiaCoveNerdFont-Regular.ttf";
pub const NERD_FONTS_VERSION: &str = "3.5.1";

/// One bundled component: its version and `file name → lowercase SHA-256`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bundle {
    pub version: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub omp: Bundle,
    pub clink: Bundle,
    pub fonts: Bundle,
}

impl Manifest {
    pub fn bundle(&self, component: Component) -> &Bundle {
        match component {
            Component::Omp => &self.omp,
            Component::Clink => &self.clink,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error("the bundled files are missing: {0} (run scripts/fetch-resources.ps1)")]
    Missing(String),
    #[error("resources/manifest.json is not valid: {0}")]
    Invalid(String),
    #[error("resources/manifest.json is for {component} {found}, but this build pins {pinned}")]
    WrongVersion {
        component: &'static str,
        found: String,
        pinned: &'static str,
    },
    #[error("{0} is missing")]
    FileMissing(PathBuf),
    #[error("{0} does not match the pinned hash")]
    FileChanged(PathBuf),
    #[error("{0}")]
    Io(String),
}

impl ResourceError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        "E_RUNTIME_MISSING"
    }
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Where the bundled files are: `<exe dir>\resources`, or `--resources <dir>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resources {
    pub root: PathBuf,
}

impl Resources {
    pub fn dir(&self, component: Component) -> PathBuf {
        match component {
            Component::Omp => self.root.join("omp"),
            Component::Clink => self.root.join("clink"),
        }
    }

    /// The bundled font file for `family`; only the Nerd Font is bundled.
    pub fn font_file(&self, family: &str) -> Option<PathBuf> {
        family
            .eq_ignore_ascii_case(NERD_FONT_FAMILY)
            .then(|| self.root.join("fonts").join(NERD_FONT_FILE))
    }

    /// Reads `manifest.json` and checks it pins the versions this build expects.
    pub fn manifest(&self, fs: &dyn Fs) -> Result<Manifest, ResourceError> {
        let path = self.root.join(MANIFEST);
        let bytes = fs
            .read(&path)
            .map_err(|e| ResourceError::Io(e.to_string()))?
            .ok_or_else(|| ResourceError::Missing(path.display().to_string()))?;
        let m: Manifest =
            serde_json::from_slice(&bytes).map_err(|e| ResourceError::Invalid(e.to_string()))?;
        for (component, found, pinned) in [
            ("Oh My Posh", &m.omp.version, OMP_VERSION),
            ("Clink", &m.clink.version, CLINK_VERSION),
            ("Nerd Fonts", &m.fonts.version, NERD_FONTS_VERSION),
        ] {
            if found != pinned {
                return Err(ResourceError::WrongVersion {
                    component,
                    found: found.clone(),
                    pinned,
                });
            }
        }
        Ok(m)
    }
}

/// Checks that every file of `bundle` sits in `dir` with its pinned hash.
pub fn check_bundle(fs: &dyn Fs, dir: &Path, bundle: &Bundle) -> Result<(), ResourceError> {
    for (name, hash) in &bundle.files {
        let path = dir.join(name);
        let bytes = fs
            .read(&path)
            .map_err(|e| ResourceError::Io(e.to_string()))?
            .ok_or_else(|| ResourceError::FileMissing(path.clone()))?;
        if sha256_hex(&bytes) != *hash {
            return Err(ResourceError::FileChanged(path));
        }
    }
    Ok(())
}

/// A tiny bundle on a fake disk: one or two files per component with their
/// real hashes, and a manifest that pins this build's versions.
#[cfg(any(test, feature = "fake"))]
pub fn fake_resources(fs: &crate::adapters::fake::FakeFs, root: &Path) -> Resources {
    let files = |dir: &str, name: &str, body: &[u8]| {
        fs.put(root.join(dir).join(name), body);
        BTreeMap::from([(name.to_string(), sha256_hex(body))])
    };
    let omp = files("omp", "oh-my-posh.exe", b"omp-binary");
    let mut clink = files("clink", "clink_x64.exe", b"clink-exe");
    clink.extend(files("clink", "clink_dll_x64.dll", b"clink-dll"));
    let fonts = files("fonts", NERD_FONT_FILE, b"font-bytes");
    let m = Manifest {
        omp: Bundle {
            version: OMP_VERSION.into(),
            files: omp,
        },
        clink: Bundle {
            version: CLINK_VERSION.into(),
            files: clink,
        },
        fonts: Bundle {
            version: NERD_FONTS_VERSION.into(),
            files: fonts,
        },
    };
    fs.put(
        root.join(MANIFEST),
        serde_json::to_vec_pretty(&m).unwrap_or_default(),
    );
    Resources {
        root: root.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;

    #[test]
    fn sha256_matches_the_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn manifest_loads_and_bundles_check_out() {
        let fs = FakeFs::new();
        let res = fake_resources(&fs, Path::new(r"C:\app\resources"));
        let m = res.manifest(&fs).unwrap();
        check_bundle(&fs, &res.dir(Component::Clink), m.bundle(Component::Clink)).unwrap();
        fs.put(r"C:\app\resources\clink\clink_dll_x64.dll", b"tampered");
        assert!(matches!(
            check_bundle(&fs, &res.dir(Component::Clink), &m.clink),
            Err(ResourceError::FileChanged(_))
        ));
        fs.remove_file(Path::new(r"C:\app\resources\omp\oh-my-posh.exe"))
            .unwrap_or(());
        assert!(matches!(
            check_bundle(&fs, &res.dir(Component::Omp), &m.omp),
            Err(ResourceError::FileMissing(_))
        ));
    }

    #[test]
    fn missing_or_mismatched_manifests_are_refused() {
        let fs = FakeFs::new();
        let res = Resources {
            root: PathBuf::from(r"C:\nowhere"),
        };
        assert!(matches!(res.manifest(&fs), Err(ResourceError::Missing(_))));
        let res = fake_resources(&fs, Path::new(r"C:\r"));
        let text = String::from_utf8(fs.file(r"C:\r\manifest.json").unwrap())
            .unwrap()
            .replace(OMP_VERSION, "31.4.0");
        fs.put(r"C:\r\manifest.json", text);
        assert!(matches!(
            res.manifest(&fs),
            Err(ResourceError::WrongVersion {
                component: "Oh My Posh",
                ..
            })
        ));
        assert_eq!(res.manifest(&fs).unwrap_err().code(), "E_RUNTIME_MISSING");
    }

    #[test]
    fn only_the_nerd_font_is_bundled() {
        let res = Resources {
            root: PathBuf::from(r"C:\r"),
        };
        assert_eq!(
            res.font_file("caskaydiacove nf"),
            Some(PathBuf::from(
                r"C:\r\fonts\CaskaydiaCoveNerdFont-Regular.ttf"
            ))
        );
        assert_eq!(res.font_file("Cascadia Mono"), None);
    }
}
