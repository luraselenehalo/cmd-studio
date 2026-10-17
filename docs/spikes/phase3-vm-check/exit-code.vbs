Option Explicit
' CMD Studio test VMs only. Runs one command the way an NSIS ExecWait does (the uninstaller runs
' "cmdstudio.exe --cleanup"; spec 6.3): hidden, with no console, waiting for the exit, and writes
' the exit code to a file. Run it with wscript.exe, which has no console of its own:
'   wscript.exe //B //Nologo exit-code.vbs <result-file> <exe> [<argument> ...]
' Every argument is put in double quotes when the command is built, so a path with a space (the
' Thai test user's profile has one) reaches the program as one argument. That suits a program
' that reads its command line by the usual rules, as cmdstudio.exe does; cmd.exe does not read
' its own switches that way, so do not run cmd.exe through this. An argument may hold
' %NAME% variables, which are expanded here, in the environment of the user this runs as, so that
' the host never has to type a Thai path:
'   exit-code.vbs C:\cmdstudio\out\phase3\exit-cleanup.txt C:\cmdstudio\phase3\app\cmdstudio.exe
'                 --cleanup --data-dir %LOCALAPPDATA%\ai.resincore.cmdstudio
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

Dim resultFile, command, i, argument, code, report, outStream
resultFile = WScript.Arguments(0)
command = ""
For i = 1 To WScript.Arguments.Count - 1
  argument = sh.ExpandEnvironmentStrings(WScript.Arguments(i))
  If i > 1 Then command = command & " "
  command = command & """" & argument & """"
Next

On Error Resume Next
code = sh.Run(command, 0, True)
If Err.Number <> 0 Then
  report = "exit=error" & vbCrLf & "could not run (error " & Err.Number & "): " & Err.Description & vbCrLf & command
  Err.Clear
Else
  report = "exit=" & code & vbCrLf & command
End If
On Error GoTo 0

Set outStream = fso.CreateTextFile(resultFile, True, True)
outStream.Write report & vbCrLf
outStream.Close
