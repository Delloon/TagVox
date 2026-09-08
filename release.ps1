<#
.SYNOPSIS
    Builds a clean release package for uploading to GitHub Releases.

.DESCRIPTION
    - Fully rebuilds the project (cargo clean + cargo build --release) to
      guarantee no leftover token or other build junk ends up in the archive.
    - Deletes token.json / config.json wherever they might accidentally
      exist (project root, target/release) -- these files contain secrets
      or local settings and must never be published.
    - Collects the exe, phrases.json, sounds/, README.md and LICENSE into
      a dist/ folder, then zips everything into
      <package_name>-v<version>-windows-x64.zip next to the script.

.USAGE
    Run from the project root (same folder as Cargo.toml):
        powershell -ExecutionPolicy Bypass -File .\release.ps1

    Faster rebuild without full cargo clean (e.g. while debugging the
    script itself):
        powershell -ExecutionPolicy Bypass -File .\release.ps1 -SkipClean
#>

param(
    [switch]$SkipClean
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Text)
    Write-Host ""
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Write-Warn {
    param([string]$Text)
    Write-Host "!! $Text" -ForegroundColor Yellow
}

# --- 0. Make sure we are in the project root ---------------------------------
$root = $PSScriptRoot
Set-Location $root

if (-not (Test-Path "Cargo.toml")) {
    throw "Cargo.toml not found in $root. Run this script from the project root."
}

# --- 1. Read package name and version from Cargo.toml ------------------------
Write-Step "Reading Cargo.toml"

$cargoToml = Get-Content "Cargo.toml" -Raw

$nameMatch = [regex]::Match($cargoToml, '(?m)^\s*name\s*=\s*"([^"]+)"')
if (-not $nameMatch.Success) {
    throw "Could not find the name field in Cargo.toml"
}
$pkgName = $nameMatch.Groups[1].Value

$versionMatch = [regex]::Match($cargoToml, '(?m)^\s*version\s*=\s*"([^"]+)"')
$pkgVersion = if ($versionMatch.Success) { $versionMatch.Groups[1].Value } else { "0.0.0" }

Write-Host "Package: $pkgName, version: $pkgVersion"

# --- 2. Remove secrets that might be left over from previous runs ------------
Write-Step "Removing token.json / config.json if they exist anywhere"

$secretPaths = @(
    "token.json",
    "config.json",
    "target\release\token.json",
    "target\release\config.json",
    "target\debug\token.json",
    "target\debug\config.json"
)
foreach ($p in $secretPaths) {
    if (Test-Path $p) {
        Remove-Item $p -Force
        Write-Warn "Removed: $p"
    }
}

# --- 3. Full rebuild -----------------------------------------------------------
if (-not $SkipClean) {
    Write-Step "cargo clean (guarantees a fully clean build)"
    cargo clean
    if ($LASTEXITCODE -ne 0) { throw "cargo clean failed" }
}
else {
    Write-Warn "Skipping cargo clean (-SkipClean flag)"
}

Write-Step "cargo build --release"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }

# --- 4. Make sure the exe and resources are in place --------------------------
$exePath = "target\release\$pkgName.exe"
if (-not (Test-Path $exePath)) {
    throw "Compiled file not found: $exePath"
}

# Double-check for secrets AFTER the build too -- the first run of the exe
# has not happened yet, but the check does not hurt.
foreach ($p in @("target\release\token.json", "target\release\config.json")) {
    if (Test-Path $p) {
        Remove-Item $p -Force
        Write-Warn "Removed after build: $p"
    }
}

# --- 5. Assemble the dist/ folder ----------------------------------------------
Write-Step "Assembling dist\ folder"

$distDir = Join-Path $root "dist"
if (Test-Path $distDir) {
    Remove-Item $distDir -Recurse -Force
}
New-Item -ItemType Directory -Path $distDir | Out-Null

Copy-Item $exePath -Destination $distDir

$optionalCopies = @("phrases.json", "README.md", "LICENSE")
foreach ($f in $optionalCopies) {
    $src = Join-Path "target\release" $f
    if (-not (Test-Path $src)) {
        $src = Join-Path $root $f
    }
    if (Test-Path $src) {
        Copy-Item $src -Destination $distDir
    }
    else {
        Write-Warn "File not found, skipped: $f"
    }
}

$soundsSrc = "target\release\sounds"
if (Test-Path $soundsSrc) {
    Copy-Item $soundsSrc -Destination (Join-Path $distDir "sounds") -Recurse
}
else {
    Write-Warn "sounds folder not found in target\release, skipped"
}

# Final check: dist/ must not contain any secrets
foreach ($f in @("token.json", "config.json")) {
    $p = Join-Path $distDir $f
    if (Test-Path $p) {
        Remove-Item $p -Force
        Write-Warn "Removed from dist: $f"
    }
}

# --- 6. Pack into a zip ---------------------------------------------------------
Write-Step "Packing into a zip"

$zipName = "$pkgName-v$pkgVersion-windows-x64.zip"
$zipPath = Join-Path $root $zipName

if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}

Compress-Archive -Path (Join-Path $distDir "*") -DestinationPath $zipPath

# --- 7. Done ---------------------------------------------------------------------
Write-Step "Done"
Write-Host "Archive ready for GitHub Releases:" -ForegroundColor Green
Write-Host "  $zipPath" -ForegroundColor Green
Write-Host ""
Write-Host "Contents:" -ForegroundColor Green
Get-ChildItem $distDir -Recurse | ForEach-Object { Write-Host "  $($_.FullName.Substring($distDir.Length + 1))" }