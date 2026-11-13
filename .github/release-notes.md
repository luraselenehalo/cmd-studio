CMD Studio @VERSION@ — a pre-release for testers.

**This build is not code-signed.** Windows SmartScreen will warn when you run the installer: choose "More info", then "Run anyway". CMD Studio will be signed through SignPath Foundation once the project qualifies; even then SmartScreen's trust builds up over time (see the README).

**What the installer does:** installs CMD Studio for your user only (no administrator rights) into `%LOCALAPPDATA%\CMD Studio`, with Oh My Posh, Clink and the CaskaydiaCove Nerd Font inside; adds that folder to your Path so `cmdstudio` works in cmd; puts "CMD Studio" and "CMD Studio Recovery" in the Start menu; starts the app. x64 Windows 10 (version 2004 and later) and Windows 11 only; ARM64 is refused.

**Uninstalling** puts everything back (AutoRun, Windows Terminal's settings, the font, your Path) and keeps your themes unless you tick the box to delete the app's data.

**Updating:** CMD Studio checks for a new version each time it starts (Settings > Updates: Automatic, Tell me first, or Off), installs it when you close the app, and tells you which version it is now. To upgrade by hand instead, run the new installer and, on its "Already Installed" page, choose **Do not uninstall**: your theme and your settings stay. "Uninstall before installing", the default, puts everything back first, and the setup wizard runs again.

**Bundled:** Clink 1.9.34 (GPL-3.0; source: https://github.com/chrisant996/clink/tree/v1.9.34), Oh My Posh 31.5.0 (MIT), CaskaydiaCove Nerd Font from Nerd Fonts 3.5.1 (SIL OFL 1.1). CMD Studio itself is MIT. The credits page in the app lists the color schemes.

SHA-256 of the installer: `@SHA256@`
