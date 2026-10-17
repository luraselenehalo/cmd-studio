Option Explicit
' CMD Studio test VMs only. Writes the AutoRun value of cmd for the user it runs as (the value
' CMD Studio's apply sets and its undo puts back) to a text file, with no console, so that it can
' be read on a Terminal that is still never-opened. Run it with wscript.exe, on the user's own
' desktop (typed with Win+R), where HKCU is the user's:
'   wscript.exe //B //Nologo read-autorun.vbs <out-file>
' The file is UTF-16 text with one line: "AutoRun: (absent)", or "AutoRun: [REG_SZ] <value>" /
' "AutoRun: [REG_EXPAND_SZ] <value>", or "AutoRun: (read failed)". A REG_EXPAND_SZ value is shown
' expanded. The type is shown because the undo has to put it back too (exec.rs same_state compares
' type and bytes).
' "(absent)" is printed only when the registry was read and there is no AutoRun value (or no
' Command Processor key at all). Every failure, whether the WMI service, the call that lists the
' values or the call that reads the AutoRun value, prints "(read failed)": a failed read is never
' taken for an absent value. Two "(read failed)" lines are equal as text and prove nothing, so
' protocol.md reads a value right after an apply to see that this script can read one.
' It quits with exit code 99 on a machine whose name does not start with CMDSTUDIO-.

Const HKCU = &H80000001
Const KEY = "Software\Microsoft\Command Processor"
Const KEY_NOT_FOUND = 2

Dim net, fso, reg, names, types, i, kind, value, text, outStream, rc, valueRc, failed
Set net = CreateObject("WScript.Network")
If UCase(Left(net.ComputerName, 10)) <> "CMDSTUDIO-" Then WScript.Quit 99
If WScript.Arguments.Count < 1 Then WScript.Quit 98
Set fso = CreateObject("Scripting.FileSystemObject")

text = "AutoRun: (absent)"
failed = False
On Error Resume Next
Set reg = GetObject("winmgmts:{impersonationLevel=impersonate}!\\.\root\default:StdRegProv")
If Err.Number <> 0 Then
  failed = True
  Err.Clear
Else
  names = Empty
  types = Empty
  rc = reg.EnumValues(HKCU, KEY, names, types)
  If Err.Number <> 0 Then
    failed = True
    Err.Clear
  ElseIf rc <> 0 And rc <> KEY_NOT_FOUND Then
    failed = True
  ElseIf rc = 0 Then
    If IsArray(names) Then
      For i = 0 To UBound(names)
        If LCase(names(i)) = "autorun" Then
          value = ""
          valueRc = 0
          If types(i) = 2 Then
            kind = "REG_EXPAND_SZ"
            valueRc = reg.GetExpandedStringValue(HKCU, KEY, names(i), value)
          ElseIf types(i) = 1 Then
            kind = "REG_SZ"
            valueRc = reg.GetStringValue(HKCU, KEY, names(i), value)
          Else
            kind = "type " & types(i)
          End If
          If Err.Number <> 0 Or valueRc <> 0 Then
            failed = True
            Err.Clear
          Else
            text = "AutoRun: [" & kind & "] " & value
          End If
        End If
      Next
    End If
  End If
End If
On Error GoTo 0
If failed Then text = "AutoRun: (read failed)"

Set outStream = fso.CreateTextFile(WScript.Arguments(0), True, True)
outStream.Write text & vbCrLf
outStream.Close
