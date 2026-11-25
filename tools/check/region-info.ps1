# Window region of the running VideoSqueeze main window: 0 none/error, 1 empty, 2 simple, 3 complex (shaped).
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TkRgn {
    [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
    [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr h, IntPtr r);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr o);
}
'@
$proc = Get-Process VideoSqueeze -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$h = $proc.MainWindowHandle
$rgn = [TkRgn]::CreateRectRgn(0, 0, 0, 0)
$kind = [TkRgn]::GetWindowRgn($h, $rgn)
[TkRgn]::DeleteObject($rgn) | Out-Null
$ex = [TkRgn]::GetWindowLong($h, -20)
"hwnd=$h region=$kind exstyle=0x{0:X8}" -f $ex
