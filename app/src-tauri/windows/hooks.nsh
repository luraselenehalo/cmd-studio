; CMD Studio's installer hooks (spec §6.1, §6.3, phase 4a). Tauri's installer.nsi inserts each
; macro at its point: PREINSTALL before any file is written, POSTINSTALL after the files and
; Tauri's own shortcuts, PREUNINSTALL before Tauri's app-running check and before any file is
; deleted, POSTUNINSTALL after Tauri's deletes. $UpdateMode is 1 only when the installer itself
; was started with /UPDATE (Tauri's updater, phase 4b): only then does it skip the old
; uninstaller (template :27004-27006) or append /UPDATE to its command line (:27041-27044). A
; newer version installed over an older one by hand runs the old uninstaller without /UPDATE,
; so with $UpdateMode = 0: the full cleanup runs, the theme comes off and the app data goes
; (themes\ kept), and the new version's wizard starts again (spec appendix B, 2026-11-04).
; POSTUNINSTALL ends the uninstall. Everything typed here is English: it is what a person sees
; in the installer's boxes. The program calls go through the console exe cmdstudio.com with nsExec, so
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
; new process, so the hooks use the folder only in POSTUNINSTALL. The template defines no
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
  ; The console exe (spec §5.7): Tauri wrote it under its cargo name, and here it becomes
  ; cmdstudio.com, which cmd picks for a bare `cmdstudio` (PATHEXT). A .com already there (an
  ; install over an older version) goes first, because Rename does not replace a file. Delete
  ; sets no error for a file that is not there, and a .com it cannot delete makes the Rename
  ; fail too, so one check covers both.
  ClearErrors
  Delete "$INSTDIR\cmdstudio.com"
  Rename "$INSTDIR\cmdstudio-cli.exe" "$INSTDIR\cmdstudio.com"
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio is installed, but its console program could not be put in place as $INSTDIR\cmdstudio.com, so typing cmdstudio in cmd may not work. The app itself (cmdstudio.exe, in the Start menu) still runs. Installing CMD Studio again may fix this." /SD IDOK
  ${EndIf}
  ; The Recovery shortcut beside the app's, in the same Start-menu folder (spec §6.1, recovery
  ; spec §4.4): cmdstudio.exe with --recovery and nothing else. CreateShortcut replaces an
  ; existing .lnk, so an install over an older version gets the new one. Under /NS Tauri makes
  ; no shortcut (CreateOrUpdateStartMenuShortcut returns when $NoShortcutMode = 1 unless it
  ; migrates from WiX, template :27639-27643), and neither does this hook.
  ${If} $WixMode = 1
  ${OrIf} $NoShortcutMode <> 1
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\CMD Studio Recovery.lnk" "$INSTDIR\cmdstudio.exe" "--recovery" "$INSTDIR\cmdstudio.exe" 0 SW_SHOWNORMAL "" "Opens CMD Studio in recovery mode: puts back a cut-off change and pauses the theme"
  ${EndIf}
  ; The install folder on the user's Path, so that cmd finds `cmdstudio` (spec §5.7). When it is
  ; there already the command changes nothing. The result is text: "error" when the program
  ; could not start, "timeout" when it did not end, else its exit code.
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
  ; Before any file is deleted, and only when the uninstaller was not started with /UPDATE (spec
  ; §6.3): with $UpdateMode = 1 (Tauri's updater, phase 4b) a cleanup would undo the user's theme
  ; (G10), and the new installer's POSTINSTALL writes the .com and the shortcut again anyway. A
  ; manual install over an older version comes here with $UpdateMode = 0 (see the top).
  ${If} $UpdateMode <> 1
    ; Tauri's own check runs only after this hook: a running app is closed (or the uninstall
    ; stopped) before the cleanup, so the app cannot write its state back over the cleanup. If
    ; the user cancels, nothing has run yet. The same macro and arguments as Tauri's
    ; (template :27482, utils.nsh's !macro CheckIfAppIsRunning executablePath productName).
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ; --cleanup puts the machine back, with --remove-themes when the "delete app data" box is
    ; ticked (then NSIS deletes the data folder itself).
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
      ; Not everything is back (3: some steps failed, 1 or 2: refused, error: the program could
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
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; After Tauri's deletes, under the same guard as PREUNINSTALL: ours, which Tauri's list does
  ; not hold under these names, the Recovery shortcut and the console exe (Tauri's Delete of
  ; cmdstudio-cli.exe finds nothing: the rename took it). $AppStartMenuFolder is set here: Tauri
  ; reads it back with MUI_STARTMENU_GETFOLDER (template :27527) inside its own
  ; $UpdateMode <> 1 block, before this hook. Tauri's RMDir of the Start-menu folder and of
  ; $INSTDIR ran while these two files were still there, so each folder is removed here, which
  ; RMDir does only when it is empty.
  ${If} $UpdateMode <> 1
    Delete "$SMPROGRAMS\$AppStartMenuFolder\CMD Studio Recovery.lnk"
    RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    Delete "$INSTDIR\cmdstudio.com"
    RMDir "$INSTDIR"
  ${EndIf}
!macroend
