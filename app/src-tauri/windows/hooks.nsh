; CMD Studio's installer hooks (spec §6.1, §6.3, phase 4a). Tauri's installer.nsi inserts each
; macro at its point: PREINSTALL before any file is written, POSTINSTALL after the files and
; Tauri's own shortcuts, PREUNINSTALL before any file is deleted (in update mode too: the
; installer runs the old uninstaller with /UPDATE, and $UpdateMode is 1 then, G10),
; POSTUNINSTALL after. Everything typed here is English: it is what a person sees in the
; installer's boxes. The program calls go through the console exe cmdstudio.com with nsExec, so
; no console window flashes and the exit code and the output come back on the stack
; (ExecToStack pushes the exit code, then the output; "error" when the program could not start).
; Rust edits the user's Path (--add-to-path, --remove-from-path), never this script: NSIS strings
; hold 1024 characters and a longer Path would be cut. The Start-menu folder is
; $AppStartMenuFolder, the variable Tauri's own shortcut uses (the installer's Start-menu page
; lets the user change the folder; the uninstaller reads it back), never the literal name.
;
; Read on 2026-11-05 from the installer.nsi template embedded in @tauri-apps/cli 2.12.3 (plan
; 4a's gate G-NSI): the four hook names, Var UpdateMode, Var AppStartMenuFolder,
; Var DeleteAppDataCheckboxState, !include x64.nsh, MUI_FINISHPAGE_RUN, and the binaries loop
; that writes every non-main [[bin]] into $INSTDIR under its own name (cmdstudio-cli.exe) and
; deletes it at uninstall. The CLI downloads NSIS 3.13, whose standard plugins hold nsExec and
; whose x64.nsh has IsNativeARM64 (plan 4a's G-NSIS; makensis names anything missing).
; Reread on 2026-11-05 for Task 3: the template's Section Install runs SetOutPath $INSTDIR, then
; PREINSTALL, then the main exe, the resources and the binaries loop, the uninstaller, the
; registry, MUI_STARTMENU_WRITE_BEGIN (which fills $AppStartMenuFolder, the page's default when
; the page was skipped) with Tauri's shortcut, then POSTINSTALL. Section Uninstall runs
; PREUNINSTALL first, before CheckIfAppIsRunning and every Delete, and reads the folder with
; MUI_STARTMENU_GETFOLDER only later, inside its own $UpdateMode <> 1 block: the uninstaller is a
; new process, so PREUNINSTALL reads it itself. The template defines no
; MUI_STARTMENUPAGE_REGISTRY_* keys, so GETFOLDER answers the default folder, as it does for
; Tauri's own shortcut. Whether the bundler's binaries map holds the non-main [[bin]] is settled
; by CI's 7z listing of the first bundle (fallback F-BIN if cmdstudio-cli.exe is not in it).
!include "x64.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; x64 only (spec §1, §6.1): Clink x64 cannot inject into an ARM64 cmd. Before any file.
  ${If} ${IsNativeARM64}
    MessageBox MB_OK|MB_ICONSTOP "CMD Studio supports x64 Windows only. This computer runs Windows on ARM64, and the add-ons CMD Studio bundles (Clink) cannot run in an ARM64 cmd window. Nothing was installed." /SD IDOK
    SetErrorLevel 3
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The console exe (spec §5.7): Tauri wrote it under its cargo name; here it becomes
  ; cmdstudio.com, which cmd picks for a bare `cmdstudio` (PATHEXT). An older .com (an update)
  ; goes first, because Rename does not replace a file that is there.
  Delete "$INSTDIR\cmdstudio.com"
  Rename "$INSTDIR\cmdstudio-cli.exe" "$INSTDIR\cmdstudio.com"
  ; The Recovery shortcut beside the app's, in the same Start-menu folder (spec §6.1, recovery
  ; spec §4.4): cmdstudio.exe with --recovery and nothing else. CreateShortcut replaces an
  ; existing .lnk, so an update gets the new one.
  CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
  CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\CMD Studio Recovery.lnk" "$INSTDIR\cmdstudio.exe" "--recovery" "$INSTDIR\cmdstudio.exe" 0 SW_SHOWNORMAL "" "Opens CMD Studio in recovery mode: puts back a cut-off change and pauses the theme"
  ; The install folder on the user's Path, so that cmd finds `cmdstudio` (spec §5.7). On an
  ; update it is there already and the command changes nothing. The result is text: "error" when
  ; the program could not start, "timeout" when it did not end, else its exit code.
  nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --add-to-path'
  Pop $0
  Pop $1
  ${If} $0 == "error"
  ${OrIf} $0 == "timeout"
  ${OrIf} $0 S!= "0"
    MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio is installed, but its folder could not be added to your Path (result: $0). What it said:$\r$\n$1$\r$\nTo type cmdstudio in cmd, add $INSTDIR to your Path by hand (Settings > System > About > Advanced system settings > Environment Variables)." /SD IDOK
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Before any file is deleted, and only on a real uninstall (spec §6.3): this hook runs on an
  ; update too ($UpdateMode = 1), where a cleanup would undo the user's theme (G10) and where the
  ; new installer's POSTINSTALL writes the .com and the shortcut again anyway.
  ${If} $UpdateMode <> 1
    ; --cleanup puts the machine back; --remove-themes when the "delete app data" box is ticked
    ; (then NSIS deletes the data folder itself).
    ${If} $DeleteAppDataCheckboxState = 1
      nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --cleanup --remove-themes'
    ${Else}
      nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --cleanup'
    ${EndIf}
    Pop $0
    Pop $1
    ${If} $0 == "error"
    ${OrIf} $0 == "timeout"
    ${OrIf} $0 S!= "0"
      ; Not everything is back (3: some steps failed; 1 or 2: refused; error: the program could
      ; not start). The uninstall goes on, and the data folder stays whatever the box said: its
      ; restore points may be the only copy of the user's original settings (spec §6.3). The
      ; program that could clean up goes with this uninstall, so the way on is a new install.
      StrCpy $DeleteAppDataCheckboxState 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio could not put everything back (cleanup result: $0). What it said:$\r$\n$1$\r$\nThe uninstall goes on, and your CMD Studio data folder is kept so that this can be fixed: install CMD Studio again, close every cmd window and Windows Terminal, press Undo everything on its Help page, then uninstall it again." /SD IDOK
    ${EndIf}
    nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --remove-from-path'
    Pop $0
    Pop $1
    ${If} $0 == "error"
    ${OrIf} $0 == "timeout"
    ${OrIf} $0 S!= "0"
      MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio's folder could not be taken off your Path (result: $0). What it said:$\r$\n$1$\r$\nRemove $INSTDIR from your Path by hand (Settings > System > About > Advanced system settings > Environment Variables)." /SD IDOK
    ${EndIf}
    ; Ours, which Tauri's own list does not hold under these names: the Recovery shortcut and
    ; the console exe (Tauri's Delete of cmdstudio-cli.exe finds nothing: the rename took it).
    ; The folder is read here: Tauri's template reads it only after this hook.
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    Delete "$SMPROGRAMS\$AppStartMenuFolder\CMD Studio Recovery.lnk"
    Delete "$INSTDIR\cmdstudio.com"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Nothing: Tauri removes its shortcut, the Start-menu folder when empty, the files and
  ; $INSTDIR, and the data folder when the box was ticked (and the cleanup succeeded).
!macroend
