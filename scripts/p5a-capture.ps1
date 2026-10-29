# P5a screenshot automation: seeds a scratch data dir with one project (a prompt
# node) and an AI config pointing at a local mock server, then drives the real
# pointer to capture the settings modal (Providers + Models) and a generate flow
# (loading -> success -> error). ASCII-only on purpose (PowerShell 5.1 parses .ps1
# in the system ANSI codepage).
param(
    [string]$Exe = "D:\Dev\open-canvas-desktop\target\debug\open-canvas-app.exe",
    [string]$MockExe = "D:\Dev\open-canvas-desktop\target\debug\examples\mock_server.exe",
    [string]$OutDir = "D:\Dev\open-canvas-desktop\docs\screenshots\p5a",
    [string]$DataDir = "$env:TEMP\open-canvas-desktop\p5a-data",
    [int]$Port = 8799
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
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
    return @{ X = $origin.X; Y = $origin.Y; Width = $rect.Right - $rect.Left; Height = $rect.Bottom - $rect.Top }
}

function ToScreen($info, $x, $y) {
    $sx = $info.X + [int][math]::Round($x * ($info.Width / $LogicalW))
    $sy = $info.Y + [int][math]::Round($y * ($info.Height / $LogicalH))
    return @($sx, $sy)
}

function Move-To($info, $x, $y, [int]$steps = 8, [int]$sleepMs = 25) {
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
    Start-Sleep -Milliseconds 300
}

function Type-Text([string]$text) {
    foreach ($ch in $text.ToCharArray()) {
        $vk = [byte][char]($ch.ToString().ToUpper())
        [Win32]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 30
        [Win32]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 30
    }
    Start-Sleep -Milliseconds 150
}

function Key-CtrlA {
    [Win32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x41, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x41, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
}

function Key-Escape {
    [Win32]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [Win32]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Paste-Text([string]$text) {
    Set-Clipboard -Value $text
    Start-Sleep -Milliseconds 150
    [Win32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x56, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x56, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Win32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
}

function Shot($info, [string]$name) {
    if (-not (Test-Path -LiteralPath $OutDir)) { [void](New-Item -ItemType Directory -Path $OutDir -Force) }
    $path = Join-Path $OutDir $name
    $bmp = New-Object System.Drawing.Bitmap($info.Width, $info.Height)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($info.X, $info.Y, 0, 0, (New-Object System.Drawing.Size($info.Width, $info.Height)))
    $gfx.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host ("saved " + $path)
}

function Write-Ascii($path, $text) {
    $dir = Split-Path -Parent $path
    if (-not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir -Force) }
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.ASCIIEncoding))
}

function Wait-Port([int]$port) {
    for ($i = 0; $i -lt 60; $i++) {
        try {
            $client = New-Object System.Net.Sockets.TcpClient
            $client.Connect("127.0.0.1", $port)
            $client.Close()
            return $true
        } catch {
            Start-Sleep -Milliseconds 200
        }
    }
    return $false
}

# --- lifecycle -------------------------------------------------------------
Get-Process open-canvas-app -ErrorAction SilentlyContinue | Stop-Process -Force
Get-Process mock_server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

if (Test-Path -LiteralPath $DataDir) { Remove-Item -LiteralPath $DataDir -Recurse -Force }
[void](New-Item -ItemType Directory -Path (Join-Path $DataDir "projects") -Force)

$library = @'
{"app":"open-canvas-desktop","version":1,"groups":[],"projects":[
{"id":"demo","title":"AI demo","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}]}
'@
Write-Ascii (Join-Path $DataDir "library.json") $library

$project = @'
{"id":"demo","title":"AI demo","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z",
"nodes":[{"id":"prompt-1","type":"prompt","title":"Prompt","position":{"x":120,"y":120},
"width":340,"height":240,"metadata":{"prompt":"a calm sea at sunset","status":"idle"}}],
"connections":[],"viewport":{"x":0,"y":0,"k":1}}
'@
Write-Ascii (Join-Path $DataDir "projects\demo.json") $project

$config = @'
{"channelMode":"local","apiFormat":"openai",
"channels":[{"id":"openrouter","name":"OpenRouter",
"baseUrl":"http://127.0.0.1:__PORT__","apiKey":"mock-key","apiFormat":"openai",
"models":[{"name":"openai/gpt-image-2.5-sunburst","capability":"image"},
{"name":"deepseek/deepseek-v4-pro","capability":"text"},
{"name":"minimax/hailuo-3","capability":"video"}]}],
"imageModel":"openrouter::openai/gpt-image-2.5-sunburst",
"textModel":"openrouter::deepseek/deepseek-v4-pro",
"videoModel":"openrouter::minimax/hailuo-3",
"count":"1","quality":"auto","size":"1:1"}
'@
Write-Ascii (Join-Path $DataDir "config.json") ($config.Replace("__PORT__", "$Port"))

$mockLog = Join-Path $DataDir "mock.log"
$mock = Start-Process -FilePath $MockExe -ArgumentList $Port -PassThru -RedirectStandardOutput $mockLog -RedirectStandardError ($mockLog + ".err")
if (-not (Wait-Port $Port)) { Stop-Process -Id $mock.Id -Force -ErrorAction SilentlyContinue; throw "mock server never listened" }

$env:OPEN_CANVAS_DATA_DIR = $DataDir
$appLog = Join-Path $DataDir "app.log"
$proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardOutput $appLog -RedirectStandardError ($appLog + ".err")
Remove-Item Env:\OPEN_CANVAS_DATA_DIR -ErrorAction SilentlyContinue

$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 100; $i++) {
    Start-Sleep -Milliseconds 150
    $proc.Refresh()
    if ($proc.HasExited) { throw "app exited early" }
    $hwnd = [Win32]::FindVisibleWindow([uint32]$proc.Id, "OpenCanvas")
    if ($hwnd -ne [IntPtr]::Zero) { break }
}
if ($hwnd -eq [IntPtr]::Zero) {
    Stop-Process -Id $proc.Id -Force; Stop-Process -Id $mock.Id -Force
    throw "window never appeared"
}

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

# Detach the IME so Type-Text produces raw latin characters (no Chinese composition).
[void][Win32]::ImmAssociateContext($hwnd, [IntPtr]::Zero)

try {
    Move-To $info 640 780 2 10
    Start-Sleep -Milliseconds 300
    Shot $info "p5a-0-library.png"

    # Open the seeded project.
    Click-At $info 414 222
    Start-Sleep -Milliseconds 700
    Shot $info "p5a-1-editor.png"

    # Select the prompt node -> panel appears (prompt is seeded in the project).
    Click-At $info 530 296
    Start-Sleep -Milliseconds 500
    Shot $info "p5a-2-panel.png"

    # Generate (image) -> loading (red Stop), then success.
    Click-At $info 784 634
    Start-Sleep -Milliseconds 350
    Shot $info "p5a-3-loading.png"
    Start-Sleep -Milliseconds 1700
    Shot $info "p5a-4-success.png"

    # Info modal shows Status: success.
    Move-To $info 530 296 8 25
    Start-Sleep -Milliseconds 600
    Click-At $info 473 135
    Start-Sleep -Milliseconds 400
    Shot $info "p5a-4b-info-success.png"
    Key-Escape
    Start-Sleep -Milliseconds 300

    # Error path: replace the prompt with a failing one -> error toast.
    Click-At $info 530 524
    Start-Sleep -Milliseconds 200
    Key-CtrlA
    Paste-Text "fail now"
    Start-Sleep -Milliseconds 200
    # Blur the composer (the click that blurs is otherwise consumed), then generate.
    Click-At $info 530 296
    Start-Sleep -Milliseconds 300
    Click-At $info 784 634
    Start-Sleep -Milliseconds 350
    Shot $info "p5a-5-loading-error.png"
    Start-Sleep -Milliseconds 6400
    Shot $info "p5a-5b-error.png"

    # Settings modal: Providers, then Models.
    Click-At $info 1254 28
    Start-Sleep -Milliseconds 500
    Shot $info "p5a-6-config-providers.png"
    Click-At $info 282 206
    Start-Sleep -Milliseconds 400
    Shot $info "p5a-7-config-models.png"
    Click-At $info 60 400
    Start-Sleep -Milliseconds 300
    Shot $info "p5a-8-config-closed.png"
}
finally {
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    if (-not $mock.HasExited) { Stop-Process -Id $mock.Id -Force -ErrorAction SilentlyContinue }
}
Write-Host "done"
