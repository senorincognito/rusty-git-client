<#
.SYNOPSIS
    Builds Rusty Git Client into a standalone .exe plus Windows installers.

.DESCRIPTION
    Checks that the toolchain is installed, installs the npm dependencies if needed, runs
    "npm run tauri build" and lists what was produced. Run it through build-release.cmd (or
    "powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1"), because PowerShell's
    default policy refuses to run unsigned .ps1 files directly.

.PARAMETER Bundles
    all  (default)  the exe, an NSIS setup .exe and an MSI
    nsis            the exe and the NSIS setup .exe
    msi             the exe and the MSI
    none            only the standalone exe, no installers (fastest)

.PARAMETER SkipInstall
    Do not run "npm ci" even when node_modules is missing.

.PARAMETER Open
    Open the output folder in Explorer when the build is done.
.PARAMETER Run
    Start the app when the build is done.

.EXAMPLE
    scripts\build-release.cmd
    scripts\build-release.cmd -Bundles none -Open
#>
[CmdletBinding()]
param(
    [ValidateSet('all', 'nsis', 'msi', 'none')]
    [string]$Bundles = 'all',
    [switch]$SkipInstall,
    [switch]$Open,
    [switch]$Run
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Write-Step([string]$Message) { Write-Host ""; Write-Host "==> $Message" -ForegroundColor Cyan }

function Require-Tool([string]$Name, [string]$Hint) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "'$Name' was not found. $Hint"
    }
}

# rustup installs cargo into ~\.cargo\bin; a terminal opened before the install may not have it on PATH yet.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) {
    $env:Path = "$env:Path;$cargoBin"
}

Write-Step 'Checking prerequisites'
Require-Tool 'node'  'Install Node.js from https://nodejs.org'
Require-Tool 'npm'   'It comes with Node.js: https://nodejs.org'
Require-Tool 'cargo' 'Install Rust from https://rustup.rs (MSVC toolchain) plus the Visual Studio C++ Build Tools, then open a new terminal.'
Write-Host ("node  " + (node --version))
Write-Host ("npm   " + (npm --version))
Write-Host ("rustc " + (rustc --version))
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Warning 'git was not found. The build does not need it, but the app itself runs "git" at runtime, so install Git for Windows before using the result.'
}

if (-not (Test-Path (Join-Path $repoRoot 'node_modules'))) {
    if ($SkipInstall) {
        throw 'node_modules is missing and -SkipInstall was given. Run "npm ci" first.'
    }
    Write-Step 'Installing npm dependencies (npm ci)'
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed (exit code $LASTEXITCODE)" }
}

Write-Step "Building (bundles: $Bundles). The first build compiles all Rust dependencies and takes a few minutes."
$tauriArgs = @('run', 'tauri', 'build')
if ($Bundles -eq 'none') {
    $tauriArgs += @('--', '--no-bundle')
} elseif ($Bundles -ne 'all') {
    $tauriArgs += @('--', '--bundles', $Bundles)
}
$started = Get-Date
& npm @tauriArgs
if ($LASTEXITCODE -ne 0) { throw "The build failed (exit code $LASTEXITCODE). The messages above say why." }

Write-Step ("Done in {0:N0} seconds. Output:" -f ((Get-Date) - $started).TotalSeconds)
$release = Join-Path $repoRoot 'src-tauri\target\release'
$artifacts = @()
$artifacts += Get-ChildItem -Path $release -Filter '*.exe' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notmatch '^(build|.*-[0-9a-f]{16})' }
# Installers only if this run produced them (older ones from a previous build are not listed).
$artifacts += Get-ChildItem -Path (Join-Path $release 'bundle') -Recurse -Include '*.exe', '*.msi' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -ge $started }
foreach ($file in $artifacts) {
    Write-Host ("  {0,6:N1} MB  {1}" -f ($file.Length / 1MB), $file.FullName)
}
Write-Host ""
Write-Host 'The installers are unsigned, so Windows SmartScreen shows an "unknown publisher" warning (More info > Run anyway).'

if ($Open) {
    $folder = if ($Bundles -eq 'none') { $release } else { Join-Path $release 'bundle' }
    Start-Process explorer.exe $folder
}

if ($Run) {
    $app = Join-Path $release 'rusty-git-client.exe'
    Write-Step "Starting $app"
    Start-Process -FilePath $app
}
