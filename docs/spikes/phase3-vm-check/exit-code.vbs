Option Explicit
' CMD Studio test VMs only. Runs one command the way an NSIS ExecWait does (the uninstaller runs
' "cmdstudio.exe --cleanup"; spec 6.3): hidden, with no console, waiting for the exit, and writes
' the exit code to a file. Run it with wscript.exe, which has no console of its own:
'   wscript.exe //B //Nologo exit-code.vbs <result-file> <exe> [<argument> ...]
' Every argument is put in double quotes when the command is built, so a path with a space (the
' Thai test user's profile has one) reaches the program as one argument. That suits a program
' that reads its command line by the usual rules, as cmdstudio.exe does; cmd.exe does not read
' its own switches that way, so do not run cmd.exe through this.
' An argument may hold a token @NAME@, which is replaced by the value of the environment variable
' NAME of the user this runs as, so that the host never has to type a Thai path. The token is
' not %NAME%: the Run dialog that types this command line may expand %NAME% itself, in its own
' environment, before this script sees it; nothing touches @NAME@. The expanded value is
' inside the quotes, so a value with a space stays one argument:
'   exit-code.vbs C:\cmdstudio\out\phase3\exit-cleanup.txt C:\cmdstudio\phase3\app\cmdstudio.exe
'                 --cleanup --data-dir @LOCALAPPDATA@\ai.resincore.cmdstudio
' A variable that is not defined gives "exit=error" and nothing is run.
' The result file is UTF-16 text: the line "exit=<code>", then the command line that ran. Read it
' with Get-Content -Encoding Unicode. A program that could not be started gives "exit=error".
' The exit codes are those of crates/cli/src/lib.rs: 0 done, 1 failed or refused, 2 invalid
' input, 3 a rollback that could not finish.

Dim net, sh, fso
Set net = CreateObject("WScript.Network")
If UCase(Left(net.ComputerName, 10)) <> "CMDSTUDIO-" Then WScript.Quit 99
Set sh = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

If WScript.Arguments.Count < 2 Then WScript.Quit 98

' The argument with every @NAME@ replaced; an undefined variable is reported in `problem`.
Dim problem
problem = ""
Function Expand(text)
  Dim re, found, hit, varName, varValue, result
  Set re = New RegExp
  re.Pattern = "@([A-Za-z_][A-Za-z0-9_]*)@"
  re.Global = True
  result = text
  Set found = re.Execute(text)
  For Each hit In found
    varName = hit.SubMatches(0)
    varValue = sh.ExpandEnvironmentStrings("%" & varName & "%")
    If varValue = "%" & varName & "%" Then
      problem = "the variable " & varName & " is not defined"
    Else
      result = Replace(result, hit.Value, varValue)
    End If
  Next
  Expand = result
End Function

Dim resultFile, command, i, argument, code, report, outStream
resultFile = WScript.Arguments(0)
command = ""
For i = 1 To WScript.Arguments.Count - 1
  argument = Expand(WScript.Arguments(i))
  If i > 1 Then command = command & " "
  command = command & """" & argument & """"
Next

If problem <> "" Then
  report = "exit=error" & vbCrLf & "not run: " & problem & vbCrLf & command
Else
  On Error Resume Next
  code = sh.Run(command, 0, True)
  If Err.Number <> 0 Then
    report = "exit=error" & vbCrLf & "could not run (error " & Err.Number & "): " & Err.Description & vbCrLf & command
    Err.Clear
  Else
    report = "exit=" & code & vbCrLf & command
  End If
  On Error GoTo 0
End If

Set outStream = fso.CreateTextFile(resultFile, True, True)
outStream.Write report & vbCrLf
outStream.Close
