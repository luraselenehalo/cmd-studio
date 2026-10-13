//! Where each Windows Terminal variant keeps settings.json (spec §5.4).
use crate::adapters::{WtLocation, WtSettings, WtVariant};
use std::path::PathBuf;

pub const STORE_PACKAGE: &str = "Microsoft.WindowsTerminal_8wekyb3d8bbwe";
pub const PREVIEW_PACKAGE: &str = "Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe";

/// Locates settings under a `%LOCALAPPDATA%` root. Tests pass a temp folder.
#[derive(Clone, Debug)]
pub struct WinWt {
    local_app_data: PathBuf,
}

impl WinWt {
    pub fn new(local_app_data: PathBuf) -> Self {
        WinWt { local_app_data }
    }
}

impl WtSettings for WinWt {
    fn locations(&self) -> Vec<WtLocation> {
        let packaged = |package: &str, variant| {
            let dir = self.local_app_data.join("Packages").join(package);
            WtLocation {
                variant,
                installed: dir.is_dir(),
                settings: dir.join("LocalState").join("settings.json"),
            }
        };
        let unpackaged = self
            .local_app_data
            .join("Microsoft")
            .join("Windows Terminal");
        vec![
            packaged(STORE_PACKAGE, WtVariant::Store),
            packaged(PREVIEW_PACKAGE, WtVariant::Preview),
            WtLocation {
                variant: WtVariant::Unpackaged,
                installed: unpackaged.is_dir(),
                settings: unpackaged.join("settings.json"),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_three_variants_under_the_given_root() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp
            .path()
            .join("Packages")
            .join(STORE_PACKAGE)
            .join("LocalState");
        std::fs::create_dir_all(&store).unwrap();
        let locs = WinWt::new(tmp.path().to_path_buf()).locations();
        assert_eq!(locs.len(), 3);
        assert_eq!(
            locs[0],
            WtLocation {
                variant: WtVariant::Store,
                installed: true,
                settings: store.join("settings.json")
            }
        );
        assert!(!locs[1].installed && !locs[2].installed);
        assert_eq!(
            locs[2].settings,
            tmp.path().join(r"Microsoft\Windows Terminal\settings.json")
        );
    }
}
