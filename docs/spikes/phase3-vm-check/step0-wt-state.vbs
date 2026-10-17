Option Explicit
' Step 0 of the phase 3 VM check (protocol.md): what Windows Terminal looks like on a clean
' snapshot, read only. It starts no program and opens no console, so it cannot end the
' never-opened state that it is there to record. Run it with wscript.exe, never with a console
' host:
'   wscript.exe //B //Nologo step0-wt-state.vbs [<local-app-data> [<out-file>]]
' <local-app-data> defaults to %LOCALAPPDATA% of the user it runs as. Pass it when a program
' started through guestcontrol runs this, because that may not have the user's environment.
' <out-file> defaults to C:\cmdstudio\out\phase3\step0-wt-state.txt. It is written as UTF-16,
' because a path may hold Thai letters, so read it with Get-Content -Encoding Unicode.
' The rules at the end are the app's own: crates/core/src/adapters/windows/wt.rs says a packaged
' Terminal is "installed" when its package folder exists, and crates/core/src/detect.rs
' (never_opened) says which installed one an apply sets up. If this file and those two disagree,
' this file is the one to fix.

Const HKLM = &H80000002
Const HKCU = &H80000001
Const STORE_PACKAGE = "Microsoft.WindowsTerminal_8wekyb3d8bbwe"
Const PREVIEW_PACKAGE = "Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe"

Dim fso, sh, net, root, outFile, lines
Set fso = CreateObject("Scripting.FileSystemObject")
Set sh = CreateObject("WScript.Shell")
Set net = CreateObject("WScript.Network")
lines = ""

If WScript.Arguments.Count >= 1 Then
  root = WScript.Arguments(0)
Else
  root = sh.ExpandEnvironmentStrings("%LOCALAPPDATA%")
End If
If WScript.Arguments.Count >= 2 Then
  outFile = WScript.Arguments(1)
Else
  outFile = "C:\cmdstudio\out\phase3\step0-wt-state.txt"
End If

Sub Say(text)
  lines = lines & text & vbCrLf
End Sub

Function YesNo(flag)
  If flag Then
    YesNo = "yes"
  Else
    YesNo = "no"
  End If
End Function

' The names in a folder, folders first, or why there are none.
Function Entries(path)
  Dim folder, item, text
  If Not fso.FolderExists(path) Then
    Entries = "(folder absent)"
    Exit Function
  End If
  text = ""
  On Error Resume Next
  Set folder = fso.GetFolder(path)
  For Each item In folder.SubFolders
    text = text & item.Name & "\ "
  Next
  For Each item In folder.Files
    text = text & item.Name & " (" & item.Size & " bytes) "
  Next
  If Err.Number <> 0 Then text = text & "(listing stopped: " & Err.Description & ")"
  Err.Clear
  If text = "" Then text = "(empty)"
  Entries = Trim(text)
End Function

' The subkeys of a registry key whose name holds "WindowsTerminal": a package full name, which
' carries the version.
Function Terminals(hive, path)
  Dim reg, names, i, text
  text = ""
  On Error Resume Next
  Set reg = GetObject("winmgmts:{impersonationLevel=impersonate}!\\.\root\default:StdRegProv")
  If Err.Number <> 0 Then
    Terminals = "(registry not readable: " & Err.Description & ")"
    Err.Clear
    Exit Function
  End If
  names = Empty
  reg.EnumKey hive, path, names
  If IsArray(names) Then
    For i = 0 To UBound(names)
      If InStr(1, names(i), "WindowsTerminal", vbTextCompare) > 0 Then text = text & names(i) & " "
    Next
  End If
  Err.Clear
  If text = "" Then text = "(none)"
  Terminals = Trim(text)
End Function

' Every group under AppxAllUserStore (Applications, InboxApplications, one per user SID, ...).
Sub AllUserStore()
  Dim reg, groups, i, hits, found, base
  base = "SOFTWARE\Microsoft\Windows\CurrentVersion\Appx\AppxAllUserStore"
  found = False
  On Error Resume Next
  Set reg = GetObject("winmgmts:{impersonationLevel=impersonate}!\\.\root\default:StdRegProv")
  If Err.Number <> 0 Then
    Say "  (registry not readable: " & Err.Description & ")"
    Err.Clear
    Exit Sub
  End If
  groups = Empty
  reg.EnumKey HKLM, base, groups
  If IsArray(groups) Then
    For i = 0 To UBound(groups)
      hits = Terminals(HKLM, base & "\" & groups(i))
      If hits <> "(none)" Then
        Say "  AppxAllUserStore\" & groups(i) & ": " & hits
        found = True
      End If
    Next
  End If
  Err.Clear
  If Not found Then Say "  AppxAllUserStore: (no Windows Terminal package)"
End Sub

Sub EnsureFolder(path)
  Dim parent
  If fso.FolderExists(path) Then Exit Sub
  parent = fso.GetParentFolderName(path)
  If parent <> "" Then EnsureFolder parent
  fso.CreateFolder path
End Sub

Dim storeDir, storeState, storeFile, previewDir, previewState, previewFile, otherDir, otherFile
Dim storeInstalled, storeHas, previewInstalled, previewHas, otherInstalled, otherHas
Dim aliasFile, aliasThere, anyPresent, neverStore, neverPreview, verdict

storeDir = root & "\Packages\" & STORE_PACKAGE
storeState = storeDir & "\LocalState"
storeFile = storeState & "\settings.json"
previewDir = root & "\Packages\" & PREVIEW_PACKAGE
previewState = previewDir & "\LocalState"
previewFile = previewState & "\settings.json"
otherDir = root & "\Microsoft\Windows Terminal"
otherFile = otherDir & "\settings.json"
aliasFile = root & "\Microsoft\WindowsApps\wt.exe"

storeInstalled = fso.FolderExists(storeDir)
storeHas = fso.FileExists(storeFile)
previewInstalled = fso.FolderExists(previewDir)
previewHas = fso.FileExists(previewFile)
otherInstalled = fso.FolderExists(otherDir)
otherHas = fso.FileExists(otherFile)
aliasThere = fso.FileExists(aliasFile)

Say "Windows Terminal state, read only (step 0 of the phase 3 VM check)"
Say "read at: " & Now
Say "computer: " & net.ComputerName
Say "user: " & net.UserName
Say "local app data: " & root
Say ""
Say "Store package " & STORE_PACKAGE
Say "  package folder exists: " & YesNo(storeInstalled)
Say "  LocalState exists: " & YesNo(fso.FolderExists(storeState))
Say "  settings.json exists: " & YesNo(storeHas)
Say "  package folder holds: " & Entries(storeDir)
Say "  LocalState holds: " & Entries(storeState)
Say "Preview package " & PREVIEW_PACKAGE
Say "  package folder exists: " & YesNo(previewInstalled)
Say "  settings.json exists: " & YesNo(previewHas)
Say "  package folder holds: " & Entries(previewDir)
Say "Unpackaged folder " & otherDir
Say "  folder exists: " & YesNo(otherInstalled)
Say "  settings.json exists: " & YesNo(otherHas)
Say "wt.exe alias " & aliasFile & " exists: " & YesNo(aliasThere)
Say "Windows Terminal packages registered (the version is in the name):"
AllUserStore
Say "  HKCU AppModel repository: " & Terminals(HKCU, "Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages")
Say ""

' The rules of the app, as it reads them (WtInfo in app/src-tauri/src/backend/system.rs):
' present when any variant has a folder and a settings.json; else set up by the apply when the
' Store one has a folder and no file, or the Preview one has a folder and no file while the Store
' one has no file either (the unpackaged variant never); else not usable.
anyPresent = (storeInstalled And storeHas) Or (previewInstalled And previewHas) Or (otherInstalled And otherHas)
neverStore = storeInstalled And Not storeHas
neverPreview = previewInstalled And Not previewHas And Not storeHas
If anyPresent Then
  verdict = "present (page 2 shows wizard.check.wtPresent)"
ElseIf neverStore Or neverPreview Then
  verdict = "neverOpened (page 2 shows wizard.check.wtNeverOpened; the apply sets up settings.json)"
Else
  verdict = "not installed as the app reads it (page 2 shows wizard.check.wtMissing with the Store button when the build is 19041 or later)"
End If
Say "What the app reads from this:"
Say "  page 2 state: " & verdict
Say "  an apply sets up the Store file: " & YesNo(neverStore)
Say "  an apply sets up the Preview file: " & YesNo(neverPreview)
Say "  LocalState exists before the apply (an undo removes a LocalState only if the apply made it): " & YesNo(fso.FolderExists(storeState))
If (Not anyPresent) And (Not neverStore) And (Not neverPreview) And aliasThere Then
  Say "FINDING: the wt.exe alias is there but no package folder is: Windows Terminal is installed for this user, and the app reads it as not installed."
End If

EnsureFolder fso.GetParentFolderName(outFile)
Dim outStream
Set outStream = fso.CreateTextFile(outFile, True, True)
outStream.Write lines
outStream.Close
