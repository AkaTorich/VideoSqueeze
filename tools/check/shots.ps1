# VideoSqueeze window at several sizes after a JS line that fills it (default: demo files).
# Usage: shots.ps1 -Out <folder> [-Prepare "App.demo()"] [-Sizes "940x600,1240x780,1700x960"] [-Eval "<js before each shot>"]
param([Parameter(Mandatory = $true)][string]$Out,
      [string]$Prepare = 'App.demo()',
      [string]$Sizes = '940x600,1240x780,1700x960',
      [string]$Eval = '')
$here = $PSScriptRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Get-Process VideoSqueeze -ErrorAction SilentlyContinue | Stop-Process -Force
& (Join-Path $here 'run-debug.ps1')
Start-Sleep -Seconds 2
for ($i = 0; $i -lt 40; $i++) {
    $ready = & (Join-Path $here 'cdp.ps1') -Eval "typeof window.App + ' ' + document.visibilityState" 2>$null
    if ($ready -match 'object visible') { break }
    Start-Sleep -Milliseconds 500
}
# a dark backdrop where the window is transparent, so the shape is visible on the shot
& (Join-Path $here 'cdp.ps1') -Eval "document.documentElement.style.background='#2a2a33'; 1" | Out-Null
if ($Prepare) { & (Join-Path $here 'cdp.ps1') -Eval "Promise.resolve($Prepare).then(()=>new Promise(r=>setTimeout(r,900))).then(()=>'ready')" }
foreach ($size in $Sizes.Split(',')) {
    $wh = $size.Split('x')
    & (Join-Path $here 'cdp.ps1') -Width ([int]$wh[0]) -Height ([int]$wh[1]) -Eval "new Promise(r=>setTimeout(()=>r(1),700))" | Out-Null
    if ($Eval) { & (Join-Path $here 'cdp.ps1') -Width ([int]$wh[0]) -Height ([int]$wh[1]) -Eval "Promise.resolve($Eval).then(()=>new Promise(r=>setTimeout(r,500))).then(()=>1)" | Out-Null }
    & (Join-Path $here 'cdp.ps1') -Width ([int]$wh[0]) -Height ([int]$wh[1]) -Shot (Join-Path $Out ('videosqueeze_' + $size + '.png'))
}
Get-Process VideoSqueeze -ErrorAction SilentlyContinue | Stop-Process -Force
