# FayGer Clean Uninstaller for Windows (PowerShell)
# Default (preserves FayID vault & MeriToken state):
#   irm https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/uninstall.ps1 | iex
# Full purge (also deletes $env:USERPROFILE\.fayger\state):
#   .\scripts\uninstall.ps1 -PurgeState
param(
    [switch]$PurgeState,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host "Usage: .\uninstall.ps1 [-PurgeState]"
    Write-Host "  -PurgeState   Also delete `$env:USERPROFILE\.fayger\state (preserved by default)."
    exit 0
}

$HomeRoot = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
$InstallDir = if ($env:FAYGER_INSTALL_DIR) { $env:FAYGER_INSTALL_DIR } else { Join-Path $HomeRoot ".fayger\bin" }
$StateDir = if ($env:FAYGER_STATE_DIR) { $env:FAYGER_STATE_DIR } else { Join-Path $HomeRoot ".fayger\state" }
$BinPath = Join-Path $InstallDir "fayger.exe"
$FaygerRoot = Split-Path -Parent $InstallDir

Write-Host "==> Checking for running local fayger processes..."
$Procs = Get-Process -Name "fayger" -ErrorAction SilentlyContinue
if ($Procs) {
    $Procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Write-Host "==> Stopped running fayger process(es)."
} else {
    Write-Host "==> No running fayger process detected."
}

if (Test-Path $BinPath) {
    Remove-Item -Path $BinPath -Force
    Write-Host "==> Removed binary: $BinPath"
} else {
    Write-Host "==> Binary not present at $BinPath."
}

if ((Test-Path $InstallDir) -and ((Get-ChildItem -Path $InstallDir -Force | Measure-Object).Count -eq 0)) {
    Remove-Item -Path $InstallDir -Force -ErrorAction SilentlyContinue
}

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath) {
    $Filtered = ($UserPath -split ";") | Where-Object { $_ -and ($_ -ne $InstallDir) }
    $Joined = $Filtered -join ";"
    if ($Joined -ne $UserPath) {
        [Environment]::SetEnvironmentVariable("Path", $Joined, "User")
        Write-Host "==> Removed $InstallDir from User PATH"
    }
}

if ($PurgeState) {
    if (Test-Path $StateDir) {
        Remove-Item -Path $StateDir -Recurse -Force
        Write-Host "==> Purged state directory: $StateDir"
    }
    if ((Test-Path $FaygerRoot) -and ((Get-ChildItem -Path $FaygerRoot -Force | Measure-Object).Count -eq 0)) {
        Remove-Item -Path $FaygerRoot -Force -ErrorAction SilentlyContinue
    }
} else {
    if (Test-Path $StateDir) {
        Write-Host "==> Preserved state directory (FayID vault & MeriToken pocket kept safe): $StateDir"
        Write-Host "    Pass -PurgeState to permanently delete local state as well."
    }
}

Write-Host "==> FayGer uninstall complete."
