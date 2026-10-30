# Measures the onset jitter of the program's sound output (tasks AUD-1 and AUD-8).
#
# Runs the release build with a settings folder of its own (your settings are not touched), starts
# the free metronome at 240 BPM in sixteenths (a click every 62.5 ms), records what the default
# output device plays (WASAPI loopback) and prints how far the clicks fall from a perfect grid.
# The window takes the keyboard focus for about a minute; do not type meanwhile. Close other
# programs that make sound.
#
#   cargo build --release --example audio_probe
#   cargo build --release
#   powershell -File tools\audio_baseline.ps1 [-Exe path\to\roll-model.exe] [-Seconds 30] [-Out folder]
#
# Compare runs on the same machine, with the same background load: the numbers depend on both.

param(
    [string]$Exe = "target\release\roll-model.exe",
    [string]$Probe = "target\release\examples\audio_probe.exe",
    [int]$Seconds = 30,
    [string]$Out = (Join-Path $env:TEMP "audio_baseline")
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
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool SetForegroundWindow(System.IntPtr h);
'@

$saved = $env:APPDATA
$env:APPDATA = $appdata
try { $app = Start-Process $Exe -PassThru } finally { $env:APPDATA = $saved }

try {
    # The main window appears after the first frame; the splash needs a key to close.
    $deadline = (Get-Date).AddSeconds(30)
    while ($app.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200; $app.Refresh() }
    if ($app.MainWindowHandle -eq 0) { throw "the program's window did not appear" }
    Start-Sleep -Seconds 3
    [void][Win.User32]::SetForegroundWindow($app.MainWindowHandle)
    Start-Sleep -Milliseconds 500
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")   # closes the splash
    Start-Sleep -Seconds 1
    [void][Win.User32]::SetForegroundWindow($app.MainWindowHandle)
    [System.Windows.Forms.SendKeys]::SendWait("m")         # starts the metronome
    Start-Sleep -Seconds 4                                 # the device is open, the clicks are steady

    & $Probe record $wav $Seconds
    if ($LASTEXITCODE -ne 0) { throw "recording failed" }

    [void][Win.User32]::SetForegroundWindow($app.MainWindowHandle)
    [System.Windows.Forms.SendKeys]::SendWait("{ESC}")
} finally {
    if (-not $app.HasExited) { [void]$app.CloseMainWindow(); Start-Sleep -Seconds 1 }
    if (-not $app.HasExited) { $app.Kill() }
}

& $Probe analyze $wav 62.5
