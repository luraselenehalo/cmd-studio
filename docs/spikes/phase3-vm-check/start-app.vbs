Option Explicit
' CMD Studio test VMs only. Starts the phase 3 build of CMD Studio on the user's desktop, with
' no console window anywhere in the chain: wscript.exe is a Windows program and cmdstudio.exe is
' a Windows-subsystem exe. Run it with wscript.exe:
'   wscript.exe //B //Nologo start-app.vbs [debug [<exe>]]
' With "debug" it first sets WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
' for this script's own process, which the app inherits and nothing else does. That is the CSP
' check of protocol.md (the page's console is read from Edge at edge://inspect, spec 5.10 and
' the CSP in app/src-tauri/tauri.conf.json). It is for the VM only: the app has no devtools
' feature and none is to be added.
' <exe> defaults to C:\cmdstudio\phase3\app\cmdstudio.exe, the CI artifact's folder.

Dim net, sh, env, exe
Set net = CreateObject("WScript.Network")
If UCase(Left(net.ComputerName, 10)) <> "CMDSTUDIO-" Then WScript.Quit 99
Set sh = CreateObject("WScript.Shell")

exe = "C:\cmdstudio\phase3\app\cmdstudio.exe"
If WScript.Arguments.Count >= 2 Then exe = WScript.Arguments(1)
If WScript.Arguments.Count >= 1 Then
  If LCase(WScript.Arguments(0)) = "debug" Then
    Set env = sh.Environment("PROCESS")
    env("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") = "--remote-debugging-port=9222"
  End If
End If

' Window style 1 (normal) and no waiting: the app is the program the user sees.
sh.Run """" & exe & """", 1, False
