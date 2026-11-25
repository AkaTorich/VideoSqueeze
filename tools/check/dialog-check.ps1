# Real-screen check of the window shape through the risky moments: start (any unshaped flash?), splash,
# main window, an open file dialog (modal, disables the owner) and after the dialog is closed (WM_CLOSE).
# Usage: dialog-check.ps1 -Out <folder>
param([Parameter(Mandatory = $true)][string]$Out)
$here = $PSScriptRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TkDlg {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    // close == false: count visible dialogs; close == true: post WM_CLOSE to them
    public static int Dialogs(int pid, bool close) {
        int n = 0;
        EnumWindows((h, l) => {
            int p; GetWindowThreadProcessId(h, out p);
            if (p == pid && IsWindowVisible(h)) {
                var sb = new StringBuilder(64); GetClassName(h, sb, 64);
                if (sb.ToString() == "#32770") { n++; if (close) PostMessage(h, 0x10, IntPtr.Zero, IntPtr.Zero); }
            }
            return true;
        }, IntPtr.Zero);
        return n;
    }
}
'@
[TkDlg]::SetProcessDPIAware() | Out-Null

function Save-Screen([int]$x, [int]$y, [int]$w, [int]$h, [string]$file) {
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    "shot: $file"
}

Get-Process VideoSqueeze -ErrorAction SilentlyContinue | Stop-Process -Force
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$cx = [int]($b.Width / 2 - 660); $cy = [int]($b.Height / 2 - 420)
& (Join-Path $here 'run-debug.ps1')
# the start, frame by frame: nothing but the shaped splash may appear
$t0 = Get-Date
foreach ($ms in 300, 700, 1200, 1800, 2600) {
    while (((Get-Date) - $t0).TotalMilliseconds -lt $ms) { Start-Sleep -Milliseconds 20 }
    Save-Screen $cx $cy 1320 840 (Join-Path $Out ("screen_start_$ms.png"))
}
Start-Sleep -Seconds 2
& (Join-Path $here 'cdp.ps1') -Eval "App.demo(); 1" | Out-Null
Start-Sleep -Milliseconds 800
& (Join-Path $here 'desktop-shot.ps1') -Shot (Join-Path $Out 'screen_main.png')
& (Join-Path $here 'cdp.ps1') -Eval "(window.__TAURI__.dialog.open({ multiple: true, title: 'Add videos' }), 1)" | Out-Null
$tk = (Get-Process VideoSqueeze | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1).Id
for ($i = 0; $i -lt 50 -and [TkDlg]::Dialogs($tk, $false) -eq 0; $i++) { Start-Sleep -Milliseconds 200 }
Start-Sleep -Milliseconds 600
& (Join-Path $here 'desktop-shot.ps1') -Shot (Join-Path $Out 'screen_dialog.png')
"dialogs closed: " + [TkDlg]::Dialogs($tk, $true)
Start-Sleep -Seconds 1.5
& (Join-Path $here 'desktop-shot.ps1') -Shot (Join-Path $Out 'screen_after.png')
