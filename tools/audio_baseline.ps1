# Measures the onset jitter of the program's sound output (tasks AUD-1 and AUD-8).
#
# Runs the release build with a settings folder of its own (your settings are not touched), starts
# the free metronome at 240 BPM in sixteenths (a click every 62.5 ms, a bar of 1000 ms), records what the default
# output device plays (WASAPI loopback) and prints how far each click wanders from its usual place in the bar.
# The window takes the keyboard focus for about a minute; do not type meanwhile. Close other
# programs that make sound.
#
#   cargo build --release --example audio_probe
#   cargo build --release
#   powershell -File tools\audio_baseline.ps1 [-Exe path\to\roll-model.exe] [-Seconds 30] [-Out folder]
#                                            [-SoakMinutes 0] [-LoadThreads 0]
#
# -SoakMinutes: let the metronome run this long before recording (a long run).
# -LoadThreads: busy-loop this many processes while recording (a busy computer; 0 = none).
#
# Compare runs on the same machine, with the same background load: the numbers depend on both.

param(
    [string]$Exe = "target\release\roll-model.exe",
    [string]$Probe = "target\release\examples\audio_probe.exe",
    [int]$Seconds = 30,
    [string]$Out = (Join-Path $env:TEMP "audio_baseline"),
    [int]$SoakMinutes = 0,
    [int]$LoadThreads = 0
)

$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path $Exe).Path
$Probe = (Resolve-Path $Probe).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$wav = Join-Path $Out ("capture-{0:yyyyMMdd-HHmmss}.wav" -f (Get-Date))

# A settings folder of its own: %APPDATA%\RollModel\config.txt is read from APPDATA.
$appdata = Join-Path $Out "appdata"
New-Item -ItemType Directory -Force (Join-Path $appdata "RollModel") | Out-Null
@(
    "metro_bpm = 240", "metro_beats = 4", "metro_sub = 3",
    "click_audio = true", "click_midi = false", "click_volume = 0.5"
) | Set-Content -Encoding ASCII (Join-Path $appdata "RollModel\config.txt")

Add-Type -AssemblyName System.Windows.Forms
Add-Type -Namespace Win -Name User32 -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint pid);
'@

# Keys go to the foreground window, so they are sent only while that is the program itself.
function Send-ToApp($keys) {
    $p = [uint32]0
    [void][Win.User32]::GetWindowThreadProcessId([Win.User32]::GetForegroundWindow(), [ref]$p)
    if ($p -ne $app.Id) { throw "the program is not the foreground window (something else took the focus); nothing was sent" }
    [System.Windows.Forms.SendKeys]::SendWait($keys)
}

$saved = $env:APPDATA
$env:APPDATA = $appdata
try { $app = Start-Process $Exe -PassThru } finally { $env:APPDATA = $saved }

try {
    # The main window appears after the first frame; the splash needs a key to close.
    $deadline = (Get-Date).AddSeconds(30)
    while ($app.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200; $app.Refresh() }
    if ($app.MainWindowHandle -eq 0) { throw "the program's window did not appear" }
    Start-Sleep -Seconds 4                                 # the steps are done and the splash has the focus
    Send-ToApp "{ENTER}"                                   # closes the splash
    Start-Sleep -Seconds 1.5
    Send-ToApp "m"                                         # starts the metronome
    Start-Sleep -Seconds 4                                 # the device is open, the clicks are steady
    if ($SoakMinutes -gt 0) { Write-Host "soaking for $SoakMinutes min..."; Start-Sleep -Seconds ($SoakMinutes * 60) }
    if ($LoadThreads -gt 0) {
        Write-Host "loading $LoadThreads processes..."
        $load = 1..$LoadThreads | ForEach-Object { Start-Process powershell -ArgumentList "-NoProfile", "-Command", "while (`$true) {}" -WindowStyle Hidden -PassThru }
    }

    & $Probe record $wav $Seconds
    if ($LASTEXITCODE -ne 0) { throw "recording failed" }
    if ($load) { $load | ForEach-Object { if (-not $_.HasExited) { $_.Kill() } }; $load = $null }

    try { Send-ToApp "{ESC}" } catch { Write-Host "(the program was not in front; it is closed below)" }
} finally {
    if ($load) { $load | ForEach-Object { if (-not $_.HasExited) { $_.Kill() } } }
    if (-not $app.HasExited) { [void]$app.CloseMainWindow(); Start-Sleep -Seconds 1 }
    if (-not $app.HasExited) { $app.Kill() }
}

# One bar is 4 beats of 250 ms; the pattern repeats exactly every bar.
& $Probe analyze $wav 1000
