# Splash screen shots while VideoSqueeze starts: polls for the splash page and captures it a few times.
# Usage: splash-shots.ps1 -Out <folder>
param([Parameter(Mandatory = $true)][string]$Out)
$here = $PSScriptRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Get-Process VideoSqueeze -ErrorAction SilentlyContinue | Stop-Process -Force
& (Join-Path $here 'run-debug.ps1')
$n = 0
for ($i = 0; $i -lt 80 -and $n -lt 3; $i++) {
    try {
        $pages = Invoke-RestMethod 'http://127.0.0.1:9222/json' -ErrorAction Stop
        if ($pages | Where-Object { $_.url -match 'splash' }) {
            & (Join-Path $here 'cdp.ps1') -Target splash -Eval "document.documentElement.style.background='#2a2a33'; 1" | Out-Null
            $n++
            & (Join-Path $here 'cdp.ps1') -Target splash -Shot (Join-Path $Out ("splash_$n.png"))
            Start-Sleep -Milliseconds 350
            continue
        }
    } catch { }
    Start-Sleep -Milliseconds 100
}
Start-Sleep -Seconds 2
& (Join-Path $here 'cdp.ps1') -Eval "document.visibilityState + ' ' + document.fonts.check('16px VideoSqueeze')"
Get-Process VideoSqueeze -ErrorAction SilentlyContinue | Stop-Process -Force
