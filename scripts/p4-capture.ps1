# P4 screenshot automation: launches the Slint app against a scratch data dir,
# drives the real pointer (SetCursorPos + mouse_event), captures the client area
# to PNGs, and proves persistence by killing + relaunching the app on the same
# data dir. ASCII-only on purpose (PowerShell 5.1 parses .ps1 in the system ANSI
# codepage).
param(
    [string]$Scenario = "full",
    [string]$Exe = "D:\Dev\open-canvas-desktop\target\debug\open-canvas-app.exe",
    [string]$OutDir = "D:\Dev\open-canvas-desktop\docs\screenshots\p4",
    [string]$DataDir = "$env:TEMP\open-canvas-desktop\p4-data"
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
    // Windows only honors SetForegroundWindow from the foreground thread unless
    // the calling thread is attached to it; the ALT tap is the documented unlock.
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
        // HWND_TOPMOST = -1, SWP_NOMOVE|SWP_NOSIZE|SWP_SHOWWINDOW = 0x43
        return SetWindowPos(hwnd, new IntPtr(-1), 0, 0, 0, 0, 0x43);
    }
    // Detaches the IME from our window so typed keys arrive raw: with the
    // Microsoft Pinyin IME attached, SPACE commits a candidate and ENTER commits
    // the composition instead of reaching the app's text field.
    public static void DetachInputMethod(IntPtr hwnd) {
        ImmAssociateContext(hwnd, IntPtr.Zero);
    }
    // Posted key sequence: winit only turns WM_CHAR into text when a WM_KEYDOWN
    // is pending, and posted messages bypass the IME that would otherwise eat
    // spaces and Enter. The CHAR message carries the final character, so no
    // shift/caps handling is needed.
    public static void PostKey(IntPtr hwnd, uint vk, int ch) {
        uint scan = MapVirtualKey(vk, 0);
        IntPtr down = (IntPtr)(long)((scan << 16) | 1);
        IntPtr up = (IntPtr)(long)((scan << 16) | 1 | (1 << 30) | (long)(1u << 31));
        PostMessage(hwnd, 0x0100, (IntPtr)(long)vk, down);       // WM_KEYDOWN
        if (ch >= 0) {
            PostMessage(hwnd, 0x0102, (IntPtr)(long)ch, IntPtr.Zero); // WM_CHAR
        }
        PostMessage(hwnd, 0x0101, (IntPtr)(long)vk, up);         // WM_KEYUP
    }
    // True when the root window under this desktop point is `hwnd` — a click at
    // this point can then only reach the app, never the user's windows.
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
    # Best effort: the first real click activates the app even when a pinned
    # topmost window (Task Manager) holds the foreground lock.
}

function Mouse($flags, $data = 0) {
    [Win32]::mouse_event($flags, 0, 0, $data, [UIntPtr]::Zero)
}

# Keeps the app above other (also-topmost) windows so screenshots show it.
function Ensure-OnTop($hwnd) {
    [void][Win32]::MakeTopmost($hwnd)
    Start-Sleep -Milliseconds 80
}

# A real click may only be sent when the app is the window under that desktop
# point; otherwise the script aborts instead of disturbing other applications.
function Assert-Point($info, $x, $y) {
    $pt = ToScreen $info $x $y
    for ($i = 0; $i -lt 6; $i++) {
        if ([Win32]::PointHits($info.Hwnd, $pt[0], $pt[1])) { return }
        [void][Win32]::MakeTopmost($info.Hwnd)
        Start-Sleep -Milliseconds 250
    }
    throw ("window at " + $pt[0] + "," + $pt[1] + " is not the app; aborting instead of clicking another window")
}

function Click-At($info, $x, $y, [string]$button = "left") {
    Ensure-OnTop $info.Hwnd
    Assert-Point $info $x $y
    Move-To $info $x $y 6 18
    Start-Sleep -Milliseconds 120
    if ($button -eq "left") {
        Mouse 0x0002; Start-Sleep -Milliseconds 60; Mouse 0x0004
    } else {
        Mouse 0x0020; Start-Sleep -Milliseconds 60; Mouse 0x0040
    }
    Start-Sleep -Milliseconds 250
}

function LeftDrag($info, $x1, $y1, $x2, $y2) {
    Ensure-OnTop $info.Hwnd
    Assert-Point $info $x1 $y1
    Assert-Point $info $x2 $y2
    Move-To $info $x1 $y1 4 18
    Mouse 0x0002
    Start-Sleep -Milliseconds 90
    $steps = 14
    $from = ToScreen $info $x1 $y1
    $to = ToScreen $info $x2 $y2
    for ($i = 1; $i -le $steps; $i++) {
        $px = [int]($from[0] + ($to[0] - $from[0]) * $i / $steps)
        $py = [int]($from[1] + ($to[1] - $from[1]) * $i / $steps)
        [void][Win32]::SetCursorPos($px, $py)
        Start-Sleep -Milliseconds 25
    }
    Mouse 0x0004
    Start-Sleep -Milliseconds 250
}

# Typing goes in as posted window messages: the Microsoft Pinyin IME intercepts
# real keys (SPACE commits a candidate, ENTER commits the composition), while
# posted messages reach the text field directly.
function Type-Text($hwnd, [string]$text) {
    foreach ($ch in $text.ToCharArray()) {
        $s = $ch.ToString()
        $code = [int][char]$s
        $vk = 0
        if ($s -ge 'a' -and $s -le 'z') { $vk = $code - 32 }
        elseif ($s -ge 'A' -and $s -le 'Z') { $vk = $code }
        elseif ($s -ge '0' -and $s -le '9') { $vk = $code }
        elseif ($s -eq ' ') { $vk = 0x20 }
        else { continue }
        [Win32]::PostKey($hwnd, [uint32]$vk, $code)
        if ($s -eq ' ') { Start-Sleep -Milliseconds 90 } else { Start-Sleep -Milliseconds 50 }
    }
    Start-Sleep -Milliseconds 200
    # Enter commits the rename input (`accepted` -> save).
    [Win32]::PostKey($hwnd, 0x0D, -1)
    Start-Sleep -Milliseconds 300
}

# Two clicks inside the system double-click interval (Click-At is too slow for
# Slint's `double-clicked`).
function DoubleClick-At($info, $x, $y) {
    Ensure-OnTop $info.Hwnd
    Assert-Point $info $x $y
    Move-To $info $x $y 6 18
    Start-Sleep -Milliseconds 120
    Mouse 0x0002; Start-Sleep -Milliseconds 40; Mouse 0x0004
    Start-Sleep -Milliseconds 70
    Mouse 0x0002; Start-Sleep -Milliseconds 40; Mouse 0x0004
    Start-Sleep -Milliseconds 400
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
    # The app's stderr (surface-level diagnostics) goes into the scratch data dir
    # so it never leaks into the committed screenshots directory.
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

# --- lifecycle -------------------------------------------------------------
Stop-App
if ($Scenario -eq "full" -and (Test-Path -LiteralPath $DataDir)) {
    Remove-Item -LiteralPath $DataDir -Recurse -Force
}
$env:OPEN_CANVAS_DATA_DIR = $DataDir

$run = Start-App
$proc = $run.Proc
$hwnd = $run.Hwnd
$info = Get-ClientInfo $hwnd
Write-Host ("client " + $info.Width + "x" + $info.Height + " at " + $info.X + "," + $info.Y)
Move-To $info 640 790 2 10
Start-Sleep -Milliseconds 300

try {
    switch ($Scenario) {
        "full" {
            # 1. Empty library (no folders).
            Shot $info "p4-01-library-empty.png"

            # 2. Create the first folder from the sidebar header.
            Click-At $info 216 84
            Start-Sleep -Milliseconds 400
            Shot $info "p4-02-folder-created.png"

            # 3. New Canvas -> editor opens on the new project.
            Click-At $info 1206 84
            Start-Sleep -Milliseconds 600
            Shot $info "p4-03-editor-new-project.png"

            # 4. Double-click the canvas: a Text node appears (Rust creates it).
            DoubleClick-At $info 700 380
            Start-Sleep -Milliseconds 500
            Shot $info "p4-04-editor-node-created.png"

            # 5. Drag the node right/down so the document differs from the
            # saved baseline (a left-drag on a node moves it, not the canvas).
            LeftDrag $info 700 380 860 460
            Start-Sleep -Milliseconds 400
            Shot $info "p4-05-editor-node-moved.png"

            # 6. Export modal from the dock (JSON envelope to the exports dir).
            Click-At $info 550 748
            Start-Sleep -Milliseconds 500
            Shot $info "p4-06-export-modal.png"
            Click-At $info 841 475
            Start-Sleep -Milliseconds 600
            Shot $info "p4-07-export-done.png"
            Click-At $info 854 684
            Start-Sleep -Milliseconds 300

            # 7. Home: the autosave flushes and the library shows the card.
            Click-At $info 66 28
            Start-Sleep -Milliseconds 700
            Shot $info "p4-08-library-card.png"

            # 8. Rename the project from the card (hover reveals the actions).
            Move-To $info 414 222 8 22
            Start-Sleep -Milliseconds 350
            Click-At $info 490 168
            Start-Sleep -Milliseconds 500
            Shot $info "p4-09-rename-edit.png"
            Type-Text $hwnd "Sprint board"
            Start-Sleep -Milliseconds 500
            Shot $info "p4-10-renamed.png"

            # 8b. Open the renamed project again: the title in the editor and the
            # saved node come from the JSON on disk (the rename was persisted).
            # Clicked twice (600ms apart) so a single lost click cannot make the
            # step a no-op; the second click is a card double-click or a
            # harmless canvas press.
            Click-At $info 414 222
            Start-Sleep -Milliseconds 600
            Click-At $info 414 222
            Start-Sleep -Milliseconds 900
            Shot $info "p4-11-renamed-reopened.png"
            Click-At $info 66 28
            Start-Sleep -Milliseconds 700

            # 9. Delete the project: arm, then confirm. Neutral clicks reset the
            # click sequence between the two presses.
            Click-At $info 524 168
            Start-Sleep -Milliseconds 600
            Shot $info "p4-12-delete-armed.png"
            Click-At $info 504 168
            Start-Sleep -Milliseconds 600
            Shot $info "p4-13-after-delete.png"

            # 10. Create + open a project again (persistence subject).
            Click-At $info 1206 84
            Start-Sleep -Milliseconds 600
            DoubleClick-At $info 640 300
            Start-Sleep -Milliseconds 400
            LeftDrag $info 640 300 780 420
            Start-Sleep -Milliseconds 900
            Shot $info "p4-14-second-project.png"
            Click-At $info 66 28
            Start-Sleep -Milliseconds 700
            Shot $info "p4-15-library-two-state.png"

            # 11. Import: reuse the exported envelope as the import source.
            $exported = Get-ChildItem (Join-Path $DataDir "exports") -Filter *.json | Select-Object -First 1
            Copy-Item $exported.FullName (Join-Path $DataDir "exports\import.json") -Force
            Click-At $info 1092 84
            Start-Sleep -Milliseconds 500
            Shot $info "p4-16-import-modal.png"
            Click-At $info 841 475
            Start-Sleep -Milliseconds 600
            Shot $info "p4-17-import-done.png"
            Click-At $info 854 684
            Start-Sleep -Milliseconds 300

            # 12. Restart: kill the app, relaunch on the same data dir, and prove
            # the library + the document came back from disk.
            Stop-App
            Start-Sleep -Milliseconds 500
            $run2 = Start-App
            $proc = $run2.Proc
            $hwnd = $run2.Hwnd
            $info = Get-ClientInfo $hwnd
            Start-Sleep -Milliseconds 400
            Shot $info "p4-18-library-after-restart.png"
            Click-At $info 414 222
            Start-Sleep -Milliseconds 600
            Click-At $info 414 222
            Start-Sleep -Milliseconds 900
            Shot $info "p4-19-document-restored.png"
        }
        default { throw ("unknown scenario " + $Scenario) }
    }
}
finally {
    Start-Sleep -Milliseconds 200
    Stop-App
    Write-Host "done"
}
