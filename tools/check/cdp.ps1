# Checking VideoSqueeze's window without keyboard or mouse: WebView2's debugging port (run-debug.ps1 starts it).
# The main window by default; -Target splash picks the splash screen.

param([string]$Eval, [string]$Shot, [int]$Width = 0, [int]$Height = 0, [int]$Port = 9222, [string]$Target = '')
$ErrorActionPreference = 'Stop'

$pages = Invoke-RestMethod "http://127.0.0.1:$Port/json"
if ($Target) { $page = $pages | Where-Object { $_.type -eq 'page' -and $_.url -match $Target } | Select-Object -First 1 }
else { $page = $pages | Where-Object { $_.type -eq 'page' -and $_.url -notmatch 'splash' } | Select-Object -First 1 }
if (-not $page) { throw 'no window' }

$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ws.Options.KeepAliveInterval = [TimeSpan]::FromSeconds(30)
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
$script:id = 0

function Send-Cdp([string]$method, $params) {
    $script:id++
    $msg = @{ id = $script:id; method = $method; params = $params } | ConvertTo-Json -Depth 10 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($msg)
    $ws.SendAsync([ArraySegment[byte]]$bytes, 'Text', $true, [Threading.CancellationToken]::None).Wait()
    $buf = New-Object byte[] 1048576
    while ($true) {
        $ms = New-Object IO.MemoryStream
        do {
            $r = $ws.ReceiveAsync([ArraySegment[byte]]$buf, [Threading.CancellationToken]::None).Result
            $ms.Write($buf, 0, $r.Count)
        } while (-not $r.EndOfMessage)
        $text = [Text.Encoding]::UTF8.GetString($ms.ToArray())
        # a big screenshot is over PowerShell 5.1's 2 MB limit of ConvertFrom-Json: its data is cut out by hand
        $at = $text.IndexOf('"data":"')
        if ($text.Length -gt 1000000 -and $at -ge 0) {
            $m = [regex]::Match($text, '"id":(\d+)')
            if ($m.Success -and [int]$m.Groups[1].Value -eq $script:id) {
                $start = $at + 8
                $data = $text.Substring($start, $text.IndexOf('"', $start) - $start)
                return [pscustomobject]@{ id = $script:id; result = [pscustomobject]@{ data = $data } }
            }
            continue
        }
        $obj = $text | ConvertFrom-Json
        if ($obj.id -eq $script:id) { return $obj }
    }
}

if ($Width -gt 0) {
    Send-Cdp 'Emulation.setDeviceMetricsOverride' @{ width = $Width; height = $Height; deviceScaleFactor = 1; mobile = ($Width -lt 720) } | Out-Null
    Start-Sleep -Milliseconds 500
}
if ($Eval) {
    $res = Send-Cdp 'Runtime.evaluate' @{ expression = $Eval; awaitPromise = $true; returnByValue = $true }
    if ($res.result.exceptionDetails) { Write-Output ('ERROR: ' + ($res.result.exceptionDetails | ConvertTo-Json -Depth 6 -Compress)) }
    else { Write-Output ($res.result.result.value | ConvertTo-Json -Depth 6 -Compress) }
}
if ($Shot) {
    $res = Send-Cdp 'Page.captureScreenshot' @{ format = 'png' }
    [IO.File]::WriteAllBytes($Shot, [Convert]::FromBase64String($res.result.data))
    Write-Output "shot: $Shot"
}
if ($Width -gt 0) { Send-Cdp 'Emulation.clearDeviceMetricsOverride' @{} | Out-Null }
$ws.Dispose()
