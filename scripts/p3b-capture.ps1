# P3b screenshot automation: launches the Slint app, drives the real pointer
# (SetCursorPos + mouse_event), and captures the client area to PNGs.
# ASCII-only on purpose (PowerShell 5.1 parses .ps1 in the system ANSI codepage).
param(
    [string]$Scenario = "repro",
    [string]$Exe = "D:\Dev\open-canvas-desktop\target\debug\open-canvas-app.exe",
    [string]$OutDir = "D:\Dev\open-canvas-desktop\docs\screenshots\p3b"
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class Win32 {
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT pt);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    public struct POINT { public int X; public int Y; }
    public static IntPtr FindVisibleWindow(uint pid, string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner != pid || !IsWindowVisible(h)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(h, sb, 256);
            if (sb.ToString() == title) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@

# Logical client size of the Slint window (width: 1280px; height: 800px).
$LogicalW = 1280.0
$LogicalH = 800.0

$ErrorActionPreference = "Stop"

function Get-ClientInfo($hwnd) {
    $rect = New-Object Win32+RECT
    [void][Win32]::GetClientRect($hwnd, [ref]$rect)
    $origin = New-Object Win32+POINT
    [void][Win32]::ClientToScreen($hwnd, [ref]$origin)
    return @{
        X = $origin.X
        Y = $origin.Y
        Width = $rect.Right - $rect.Left
        Height = $rect.Bottom - $rect.Top
    }
}

# Map logical window coordinates to absolute screen pixels.
function ToScreen($info, $x, $y) {
    $sx = $info.X + [int][math]::Round($x * ($info.Width / $LogicalW))
    $sy = $info.Y + [int][math]::Round($y * ($info.Height / $LogicalH))
    return @($sx, $sy)
}

function Move-To($info, $x, $y, [int]$steps = 8, [int]$sleepMs = 25) {
    $target = ToScreen $info $x $y
    $cur = New-Object Win32+POINT
    [void][Win32]::GetCursorPos([ref]$cur)
    # Walk in small steps so Slint sees a realistic move/hover sequence.
    for ($i = 1; $i -le $steps; $i++) {
        $px = [int]($cur.X + ($target[0] - $cur.X) * $i / $steps)
        $py = [int]($cur.Y + ($target[1] - $cur.Y) * $i / $steps)
        [void][Win32]::SetCursorPos($px, $py)
        Start-Sleep -Milliseconds $sleepMs
    }
}

function Mouse($flags, $data = 0) {
    [Win32]::mouse_event($flags, 0, 0, $data, [UIntPtr]::Zero)
}

function Click-At($info, $x, $y, [string]$button = "left") {
    Move-To $info $x $y 6 20
    Start-Sleep -Milliseconds 120
    if ($button -eq "left") {
        Mouse 0x0002; Start-Sleep -Milliseconds 60; Mouse 0x0004
    } else {
        Mouse 0x0020; Start-Sleep -Milliseconds 60; Mouse 0x0040
    }
    Start-Sleep -Milliseconds 250
}

function MiddleDrag($info, $x1, $y1, $x2, $y2) {
    Move-To $info $x1 $y1 4 20
    Mouse 0x0020
    Start-Sleep -Milliseconds 80
    $steps = 12
    $from = ToScreen $info $x1 $y1
    $to = ToScreen $info $x2 $y2
    for ($i = 1; $i -le $steps; $i++) {
        $px = [int]($from[0] + ($to[0] - $from[0]) * $i / $steps)
        $py = [int]($from[1] + ($to[1] - $from[1]) * $i / $steps)
        [void][Win32]::SetCursorPos($px, $py)
        Start-Sleep -Milliseconds 25
    }
    Mouse 0x0040
    Start-Sleep -Milliseconds 200
}

function Key-Escape {
    [Win32]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [Win32]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Type-Text([string]$text) {
    foreach ($ch in $text.ToCharArray()) {
        $vk = [byte][char]($ch.ToString().ToUpper())
        [Win32]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 40
        [Win32]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 40
    }
    # Commit the IME composition as raw latin text (Microsoft Pinyin: Enter).
    Start-Sleep -Milliseconds 150
    [Win32]::keybd_event(0x0D, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [Win32]::keybd_event(0x0D, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Shot($info, [string]$name) {
    $dir = Split-Path -Parent (Join-Path $OutDir $name)
    if (-not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
    $path = Join-Path $OutDir $name
    $bmp = New-Object System.Drawing.Bitmap($info.Width, $info.Height)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($info.X, $info.Y, 0, 0, (New-Object System.Drawing.Size($info.Width, $info.Height)))
    $gfx.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host ("saved " + $path)
}

# --- lifecycle -------------------------------------------------------------
Get-Process open-canvas-app -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

$proc = Start-Process -FilePath $Exe -PassThru
$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 100; $i++) {
    Start-Sleep -Milliseconds 150
    $proc.Refresh()
    if ($proc.HasExited) { throw "app exited early" }
    $hwnd = [Win32]::FindVisibleWindow([uint32]$proc.Id, "OpenCanvas")
    if ($hwnd -ne [IntPtr]::Zero) { break }
}
if ($hwnd -eq [IntPtr]::Zero) { Stop-Process -Id $proc.Id -Force; throw "window never appeared" }

[void][Win32]::ShowWindow($hwnd, 9)
[void][Win32]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 400
if ([Win32]::GetForegroundWindow() -ne $hwnd) {
    $shell = New-Object -ComObject WScript.Shell
    [void]$shell.AppActivate($proc.Id)
    Start-Sleep -Milliseconds 300
}

$info = Get-ClientInfo $hwnd
Write-Host ("client " + $info.Width + "x" + $info.Height + " at " + $info.X + "," + $info.Y)
# Park the cursor away from the canvas so no hover state leaks between steps.
Move-To $info 640 790 2 10
Start-Sleep -Milliseconds 300

try {
    switch ($Scenario) {
        # (A) The hover-bar click bug: hovering a pill keeps the bar alive and the
        # `info` pill opens the node info modal (info + JSON view).
        "info" {
            Move-To $info 902 224 8 25
            Start-Sleep -Milliseconds 600
            Shot $info "p3b-1-hover-bar.png"
            Click-At $info 851 68
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-2-info-modal.png"
            Click-At $info 806 181
            Start-Sleep -Milliseconds 300
            Shot $info "p3b-3-info-json.png"
            Key-Escape
        }
        # (B) Prompt node composer (image mode) + its image settings popover.
        "composer" {
            MiddleDrag $info 700 600 850 600
            Click-At $info 624 272
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-4-prompt-panel.png"
            Click-At $info 669 610
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-5-prompt-image-settings.png"
            Key-Escape
        }
        # (B) Text node composer (text mode) + reasoning/count popover.
        "text" {
            MiddleDrag $info 700 600 600 390
            Click-At $info 802 338
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-6-text-panel.png"
            Click-At $info 847 676
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-7-text-settings.png"
            Click-At $info 614 676
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-8-text-model-list.png"
            Key-Escape
        }
        # (B) Video-prompt composer (video mode): settings popover + stop state.
        "video" {
            MiddleDrag $info 700 600 800 340
            Click-At $info 574 310
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-9-video-panel.png"
            Click-At $info 619 678
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-10-video-settings.png"
            Click-At $info 830 678
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-11-video-stop.png"
        }
        # (B) Image-generation config panel + its image settings popover.
        "config" {
            MiddleDrag $info 700 300 100 248
            Click-At $info 780 300
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-12-config-panel.png"
            Click-At $info 831 750
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-13-config-image-settings.png"
            Key-Escape
        }
        # (B) Video-generation config panel + its video settings popover.
        "videoconfig" {
            MiddleDrag $info 280 750 280 60
            MiddleDrag $info 900 400 450 340
            Click-At $info 900 300
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-14-video-config-panel.png"
            Click-At $info 981 652
            Start-Sleep -Milliseconds 350
            Shot $info "p3b-15-video-config-settings.png"
            Key-Escape
        }
        # (A) The other hover-bar pills fire too: duplicate, font-up, layers.
        "actions" {
            Move-To $info 902 548 8 25
            Start-Sleep -Milliseconds 600
            Click-At $info 851 392
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-17-duplicate.png"
            Click-At $info 919 392
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-18-font-up.png"
            Click-At $info 953 392
            Start-Sleep -Milliseconds 400
            Shot $info "p3b-19-layers.png"
        }
        # (B) Composer editing: type into the prompt textarea; the card body and
        # the node metadata follow (Rust owns the value).
        "typecheck" {
            MiddleDrag $info 700 600 850 600
            Click-At $info 624 272
            Start-Sleep -Milliseconds 400
            Click-At $info 700 440
            Start-Sleep -Milliseconds 250
            Type-Text " XX"
            Start-Sleep -Milliseconds 300
            Shot $info "p3b-16-prompt-typed.png"
            Key-Escape
        }
        default { throw ("unknown scenario " + $Scenario) }
    }
}
finally {
    Start-Sleep -Milliseconds 200
    Get-Process open-canvas-app -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host "done"
}
