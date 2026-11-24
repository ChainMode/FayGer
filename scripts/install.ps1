# FayGer One-Line Installer for Windows (x86_64)
# Usage: irm https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.ps1 | iex
$ErrorActionPreference = "Stop"

$Version = if ($env:FAYGER_VERSION) { $env:FAYGER_VERSION } else { "v0.3.1" }
$InstallDir = if ($env:FAYGER_INSTALL_DIR) { $env:FAYGER_INSTALL_DIR } else { Join-Path $HOME ".fayger\bin" }
$Repo = if ($env:FAYGER_GITHUB_REPO) { $env:FAYGER_GITHUB_REPO } else { "ChainMode/FayGer" }

$Asset = "fayger-x86_64-pc-windows-msvc.exe"
$BaseUrl = "https://github.com/$Repo/releases/download/$Version"
$BinUrl = "$BaseUrl/$Asset"
$SumUrl = "$BaseUrl/$Asset.sha256"

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("fayger-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null

try {
    $TmpBin = Join-Path $TmpDir $Asset
    $TmpSum = Join-Path $TmpDir "$Asset.sha256"

    Write-Host "==> Downloading FayGer $Version for Windows x86_64..."
    Invoke-WebRequest -Uri $BinUrl -OutFile $TmpBin -UseBasicParsing
    Invoke-WebRequest -Uri $SumUrl -OutFile $TmpSum -UseBasicParsing

    $SumContent = (Get-Content -Path $TmpSum -Raw).Trim()
    $ExpectedSha = ($SumContent -split "\s+")[0].ToLowerInvariant()
    $ActualSha = (Get-FileHash -Path $TmpBin -Algorithm SHA256).Hash.ToLowerInvariant()

    if ($ExpectedSha -ne $ActualSha) {
        throw "SHA-256 mismatch! Expected $ExpectedSha, got $ActualSha"
    }
    Write-Host "==> SHA-256 verified: $ActualSha"

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $DestExe = Join-Path $InstallDir "fayger.exe"
    Copy-Item -Path $TmpBin -Destination $DestExe -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathParts = if ($UserPath) { $UserPath -split ";" } else { @() }
    if ($PathParts -notcontains $InstallDir) {
        $NewUserPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "==> Added $InstallDir to User PATH"
    }

    Write-Host "==> Running installation smoke check..."
    & $DestExe version

    Write-Host ""
    Write-Host "FayGer $Version installed to $DestExe"
    Write-Host "Launch FayGer Engine + Web Console:"
    Write-Host "  fayger up --open"
}
finally {
    if (Test-Path $TmpDir) {
        Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
