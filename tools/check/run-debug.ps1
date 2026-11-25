# Starts the built VideoSqueeze.exe with WebView2's debugging port 9222, for tools\check\cdp.ps1
param([int]$Port = 9222)
$exe = Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'target\release\VideoSqueeze.exe'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process $exe
