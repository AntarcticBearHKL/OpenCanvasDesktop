# P3c screenshot automation: seeds a scratch data dir with a prompt node that has
# upstream image/text references, launches the Slint app, drives the real pointer
# and posted keys (the Microsoft Pinyin IME is detached so raw keys reach the
# field), and captures the client area to PNGs:
#   1. the `@` mention menu open,
#   2. a reference chip inserted (composer strip + node card),
#   3. the node card reference chip with the panel closed,
#   4. the chip removed by Backspace.
# ASCII-only on purpose (PowerShell 5.1 parses .ps1 in the system ANSI codepage).
param(
    [string]$Exe = "D:\Dev\open-canvas-desktop\target\debug\open-canvas-app.exe",
    [string]$OutDir = "D:\Dev\open-canvas-desktop\docs\screenshots\p3c",
    [string]$DataDir = "$env:TEMP\open-canvas-desktop\p3c-data"
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
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT pt);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT pt);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hWnd, uint gaFlags);
    [DllImport("imm32.dll")] public static extern IntPtr ImmAssociateContext(IntPtr hWnd, IntPtr hIMC);
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
    public static bool ForceForeground(IntPtr hwnd) {
        IntPtr fg = GetForegroundWindow();
        if (fg == hwnd) return true;
        uint fgPid;
        uint fgThread = GetWindowThreadProcessId(fg, out fgPid);
        uint thisThread = GetCurrentThreadId();
        AttachThreadInput(thisThread, fgThread, true);
        keybd_event(0x12, 0, 0, UIntPtr.Zero);
        keybd_event(0x12, 0, 2, UIntPtr.Zero);
        BringWindowToTop(hwnd);
        bool ok = SetForegroundWindow(hwnd);
        AttachThreadInput(thisThread, fgThread, false);
        return ok;
    }
    public static bool MakeTopmost(IntPtr hwnd) {
        return SetWindowPos(hwnd, new IntPtr(-1), 0, 0, 0, 0, 0x43);
    }
    public static void DetachInputMethod(IntPtr hwnd) {
        ImmAssociateContext(hwnd, IntPtr.Zero);
    }
    // Posted key sequence: winit turns WM_CHAR into text; posted messages bypass
    // the IME that would otherwise eat '@' and Enter. Non-text keys pass ch = -1.
    public static void PostKey(IntPtr hwnd, uint vk, int ch) {
        uint scan = MapVirtualKey(vk, 0);
        IntPtr down = (IntPtr)(long)((scan << 16) | 1);
        IntPtr up = (IntPtr)(long)((scan << 16) | 1 | (1 << 30) | (long)(1u << 31));
        PostMessage(hwnd, 0x0100, (IntPtr)(long)vk, down);
        if (ch >= 0) {
            PostMessage(hwnd, 0x0102, (IntPtr)(long)ch, IntPtr.Zero);
        }
        PostMessage(hwnd, 0x0101, (IntPtr)(long)vk, up);
    }
    public static bool PointHits(IntPtr hwnd, int x, int y) {
        POINT pt = new POINT();
        pt.X = x;
        pt.Y = y;
        IntPtr hit = WindowFromPoint(pt);
        if (hit == IntPtr.Zero) return false;
        return GetAncestor(hit, 2) == hwnd;
    }
}
"@

$LogicalW = 1280.0
$LogicalH = 800.0

$ErrorActionPreference = "Stop"

function Get-ClientInfo($hwnd) {
    $rect = New-Object Win32+RECT
    [void][Win32]::GetClientRect($hwnd, [ref]$rect)
    $origin = New-Object Win32+POINT
    [void][Win32]::ClientToScreen($hwnd, [ref]$origin)
    return @{
        Hwnd = $hwnd
        X = $origin.X
        Y = $origin.Y
        Width = $rect.Right - $rect.Left
        Height = $rect.Bottom - $rect.Top
    }
}

function ToScreen($info, $x, $y) {
    $sx = $info.X + [int][math]::Round($x * ($info.Width / $LogicalW))
    $sy = $info.Y + [int][math]::Round($y * ($info.Height / $LogicalH))
    return @($sx, $sy)
}

function Move-To($info, $x, $y, [int]$steps = 8, [int]$sleepMs = 20) {
    $target = ToScreen $info $x $y
    $cur = New-Object Win32+POINT
    [void][Win32]::GetCursorPos([ref]$cur)
    for ($i = 1; $i -le $steps; $i++) {
        $px = [int]($cur.X + ($target[0] - $cur.X) * $i / $steps)
        $py = [int]($cur.Y + ($target[1] - $cur.Y) * $i / $steps)
        [void][Win32]::SetCursorPos($px, $py)
        Start-Sleep -Milliseconds $sleepMs
    }
}

function Assert-Foreground($hwnd) {
    for ($i = 0; $i -lt 5; $i++) {
        if ([Win32]::GetForegroundWindow() -eq $hwnd) { return }
        [void][Win32]::MakeTopmost($hwnd)
        [void][Win32]::ForceForeground($hwnd)
        Start-Sleep -Milliseconds 200
    }
}

function Mouse($flags, $data = 0) {
    [Win32]::mouse_event($flags, 0, 0, $data, [UIntPtr]::Zero)
}

function Ensure-OnTop($hwnd) {
    [void][Win32]::MakeTopmost($hwnd)
    Start-Sleep -Milliseconds 80
}

function Assert-Point($info, $x, $y) {
    $pt = ToScreen $info $x $y
    for ($i = 0; $i -lt 6; $i++) {
        if ([Win32]::PointHits($info.Hwnd, $pt[0], $pt[1])) { return }
        [void][Win32]::MakeTopmost($info.Hwnd)
        Start-Sleep -Milliseconds 250
    }
    throw ("window at " + $pt[0] + "," + $pt[1] + " is not the app; aborting instead of clicking another window")
}

function Click-At($info, $x, $y) {
    Ensure-OnTop $info.Hwnd
    Assert-Point $info $x $y
    Move-To $info $x $y 6 18
    Start-Sleep -Milliseconds 120
    Mouse 0x0002; Start-Sleep -Milliseconds 60; Mouse 0x0004
    Start-Sleep -Milliseconds 250
}

# Key with no character payload (arrows, Enter, Escape, Backspace, Delete).
function Key-At($hwnd, [byte]$vk) {
    [Win32]::PostKey($hwnd, [uint32]$vk, -1)
    Start-Sleep -Milliseconds 220
}

function Type-At($hwnd) {
    [Win32]::PostKey($hwnd, 0x32, 0x40)
    Start-Sleep -Milliseconds 250
}

function Shot($info, [string]$name) {
    Ensure-OnTop $info.Hwnd
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

function Start-App() {
    if (-not (Test-Path -LiteralPath $DataDir)) { [void](New-Item -ItemType Directory -Path $DataDir -Force) }
    $logPath = Join-Path $DataDir "app-stderr.log"
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $logPath
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
    [void][Win32]::DetachInputMethod($hwnd)
    [void][Win32]::MakeTopmost($hwnd)
    Start-Sleep -Milliseconds 400
    Assert-Foreground $hwnd
    return @{ Proc = $proc; Hwnd = $hwnd }
}

function Stop-App() {
    Get-Process open-canvas-app -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 400
}

# --- seed ------------------------------------------------------------------
# A prompt node with an upstream image (data URL) and text node; these become
# the `Image 1` / `Text 1` mention candidates.
function Seed-Data {
    if (Test-Path -LiteralPath $DataDir) { Remove-Item -LiteralPath $DataDir -Recurse -Force }
    [void](New-Item -ItemType Directory -Path (Join-Path $DataDir "projects") -Force)

    $index = @"
{"app":"open-canvas-desktop","version":1,"groups":[],"projects":[{"id":"p3c-demo","title":"Mention demo","createdAt":"2026-10-17T00:00:00Z","updatedAt":"2026-10-17T00:00:00Z"}]}
"@
    $project = @"
{"id":"p3c-demo","title":"Mention demo","createdAt":"2026-10-17T00:00:00Z","updatedAt":"2026-10-17T00:00:00Z","nodes":[{"id":"image-ref-1","type":"image","title":"Image","position":{"x":64.0,"y":40.0},"width":340.0,"height":240.0,"metadata":{"content":"data:image/png;base64,cG5n","status":"success"}},{"id":"text-ref-1","type":"text","title":"Text","position":{"x":64.0,"y":340.0},"width":340.0,"height":240.0,"metadata":{"content":"A calm lake at sunrise.","status":"success","fontSize":14.0}},{"id":"prompt-1","type":"prompt","title":"Prompt","position":{"x":492.0,"y":40.0},"width":340.0,"height":240.0,"metadata":{"prompt":"","status":"idle"}}],"connections":[{"id":"conn-img-prompt","fromNodeId":"image-ref-1","toNodeId":"prompt-1"},{"id":"conn-text-prompt","fromNodeId":"text-ref-1","toNodeId":"prompt-1"}],"viewport":{"x":0.0,"y":0.0,"k":1.0}}
"@
    Set-Content -LiteralPath (Join-Path $DataDir "library.json") -Value $index -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $DataDir "projects\p3c-demo.json") -Value $project -Encoding ASCII
}

# --- lifecycle -------------------------------------------------------------
Stop-App
Seed-Data
$env:OPEN_CANVAS_DATA_DIR = $DataDir

$run = Start-App
$proc = $run.Proc
$hwnd = $run.Hwnd
$info = Get-ClientInfo $hwnd
Write-Host ("client " + $info.Width + "x" + $info.Height + " at " + $info.X + "," + $info.Y)
Move-To $info 640 790 2 10
Start-Sleep -Milliseconds 300

try {
    # Open the seeded project from the library card (double click interval).
    Click-At $info 414 222
    Start-Sleep -Milliseconds 600
    Click-At $info 414 222
    Start-Sleep -Milliseconds 900

    # Focus the prompt node: the composer panel opens under its card. The canvas
    # starts right of the 260px project side panel, so world x 492 is screen 752.
    Click-At $info 900 200
    Start-Sleep -Milliseconds 500

    # Focus the composer field and type `@`: the mention menu opens. The IME is
    # detached again because Slint enables it when the field takes focus, and an
    # active IME swallows the next real click.
    Click-At $info 900 420
    Start-Sleep -Milliseconds 400
    [void][Win32]::DetachInputMethod($hwnd)
    Type-At $hwnd
    Start-Sleep -Milliseconds 500
    Shot $info "p3c-1-mention-menu.png"

    # Arrow navigation, then Enter inserts the active reference as a chip.
    Key-At $hwnd 0x28
    Key-At $hwnd 0x26
    Key-At $hwnd 0x0D
    Start-Sleep -Milliseconds 400
    Shot $info "p3c-2-chip-inserted.png"

    # Deselect on empty canvas: the panel closes, the card chip stays. Clicked
    # twice so one lost click cannot make the step a no-op.
    [void][Win32]::DetachInputMethod($hwnd)
    Click-At $info 1100 700
    Start-Sleep -Milliseconds 300
    Click-At $info 1100 700
    Start-Sleep -Milliseconds 300
    Shot $info "p3c-3-node-card-chip.png"

    # Reselect the node, put the caret after the chip, Backspace twice: the
    # trailing space first, then the whole chip.
    Click-At $info 900 200
    Start-Sleep -Milliseconds 500
    Click-At $info 900 404
    Start-Sleep -Milliseconds 400
    Key-At $hwnd 0x08
    Key-At $hwnd 0x08
    Start-Sleep -Milliseconds 400
    Shot $info "p3c-4-backspace-removed.png"
}
finally {
    Start-Sleep -Milliseconds 200
    Stop-App
    Write-Host "done"
}
