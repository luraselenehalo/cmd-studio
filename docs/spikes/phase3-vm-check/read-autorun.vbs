Option Explicit
' CMD Studio test VMs only. Writes the AutoRun value of cmd for the user it runs as (the value
' CMD Studio's apply sets and its undo puts back) to a text file, with no console, so that it can
' be read on a Terminal that is still never-opened. Run it with wscript.exe, on the user's own
' desktop (typed with Win+R), where HKCU is the user's:
'   wscript.exe //B //Nologo read-autorun.vbs <out-file>
' The file is UTF-16 text with one line: "AutoRun: (absent)", or "AutoRun: [REG_SZ] <value>" /
' "AutoRun: [REG_EXPAND_SZ] <value>". A REG_EXPAND_SZ value is shown expanded. The type is shown
' because the undo has to put it back too (exec.rs same_state compares type and bytes).
' It quits with exit code 99 on a machine whose name does not start with CMDSTUDIO-.

Const HKCU = &H80000001
Const KEY = "Software\Microsoft\Command Processor"

Dim net, fso, reg, names, types, i, kind, value, text, outStream
Set net = CreateObject("WScript.Network")
If UCase(Left(net.ComputerName, 10)) <> "CMDSTUDIO-" Then WScript.Quit 99
If WScript.Arguments.Count < 1 Then WScript.Quit 98
Set fso = CreateObject("Scripting.FileSystemObject")

text = "AutoRun: (absent)"
On Error Resume Next
Set reg = GetObject("winmgmts:{impersonationLevel=impersonate}!\\.\root\default:StdRegProv")
If Err.Number <> 0 Then
  text = "AutoRun: (not readable: " & Err.Description & ")"
  Err.Clear
Else
  names = Empty
  types = Empty
  reg.EnumValues HKCU, KEY, names, types
  If IsArray(names) Then
    For i = 0 To UBound(names)
      If LCase(names(i)) = "autorun" Then
        value = ""
        If types(i) = 2 Then
          kind = "REG_EXPAND_SZ"
          reg.GetExpandedStringValue HKCU, KEY, names(i), value
        ElseIf types(i) = 1 Then
          kind = "REG_SZ"
          reg.GetStringValue HKCU, KEY, names(i), value
        Else
          kind = "type " & types(i)
        End If
        text = "AutoRun: [" & kind & "] " & value
      End If
    Next
  End If
  Err.Clear
End If
On Error GoTo 0

Set outStream = fso.CreateTextFile(WScript.Arguments(0), True, True)
outStream.Write text & vbCrLf
outStream.Close
