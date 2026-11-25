# Real screen capture of the VideoSqueeze main window (shape, transparency and glow as Windows draws them).
# Usage: desktop-shot.ps1 -Shot <file.png> [-Pad 24]
param([Parameter(Mandatory = $true)][string]$Shot, [int]$Pad = 24)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TkWin {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@
[TkWin]::SetProcessDPIAware() | Out-Null
$proc = Get-Process VideoSqueeze -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$r = New-Object TkWin+RECT
[TkWin]::GetWindowRect($proc.MainWindowHandle, [ref]$r) | Out-Null
$x = $r.L - $Pad; $y = $r.T - $Pad; $w = ($r.R - $r.L) + 2 * $Pad; $h = ($r.B - $r.T) + 2 * $Pad
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
$bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
"shot: $Shot  window: $($r.L),$($r.T) - $($r.R),$($r.B)"
