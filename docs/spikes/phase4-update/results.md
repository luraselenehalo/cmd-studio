# Phase 4b — the updater spike's results (Task 1)

- **Run:** <date>, commit <sha>, CI run <id> (the `cmdstudio-updater-spike` artifact), VM <A or B>.
- **U1 (download, verified, written):** <PASS and the log lines, or the finding>.
- **U2 ("Update now": silent, no uninstaller, the app back by itself as a normal start, from the home page and from the Recovery page):** <PASS, the seconds from the click to the new version and to the window, or the finding>.
- **U3 (the own spawn at close: silent, no relaunch, the exe replaced, the run record ended first):** <PASS, the seconds from the close to the new version, or the finding>.
- **U4 (the notice, its link, no re-apply with equal pins, no false crash notice, the record gone):** <…>.
- **U5 (a manifest that is not JSON, and one with the same version):** <…; the kind the bad manifest logged (`offline` or `manifest`)>.
- **U6 (a by-hand upgrade keeps the theme through "Do not uninstall"; the page's words):** <the two radios' exact texts>.
- **U7 (Off makes no request; a refused connection is silent):** <…>.
- **Decision:** the install moment is **<primary | F2 | F3>** and the "now" path is **<as planned | F-NOW>**, because <one sentence>. Task 3's last step and Task 8 read this line.
- **Screenshots** (`spikes\vm\tmp\<vm>-p4b-*.png`): <labels>.
