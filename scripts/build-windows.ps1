<#
.SYNOPSIS
  Build a local Windows x64 test installer. Run on Windows; see docs/WINDOWS.md.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1
#>
[CmdletBinding()]
param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitProcess) {
    throw 'Run this script in 64-bit PowerShell on Windows. See docs/WINDOWS.md.'
}
foreach ($Command in @('npm.cmd', 'cargo', 'rustup', 'rustc')) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) { throw "Missing $Command. See docs/WINDOWS.md for prerequisites." }
}
function Invoke-Checked([string]$Command, [string[]]$Arguments) {
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE." }
}
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Target = 'x86_64-pc-windows-msvc'
Push-Location $Root
try {
    # The MSVC target is required by Tauri/WebView2 even if another Rust target is installed.
    Invoke-Checked 'rustup' @('target', 'add', $Target)
    Invoke-Checked 'npm.cmd' @('ci')
    & (Join-Path $PSScriptRoot 'fetch-engine.ps1')
    foreach ($Directory in @('engine', 'engine-baseline')) {
        # Catch missing VC++ runtime/DLLs before producing an installer.
        Invoke-Checked (Join-Path $Root "src-tauri/resources/$Directory/llama-server.exe") @('--version')
    }
    if (-not $SkipChecks) {
        Invoke-Checked 'npm.cmd' @('run', 'check')
        Invoke-Checked 'npm.cmd' @('run', 'test')
        Invoke-Checked 'cargo' @('fmt', '--all', '--check')
        Invoke-Checked 'cargo' @('test', '--workspace', '--target', $Target)
        Invoke-Checked 'cargo' @('check', '-p', 'tnn-local-ai', '--target', $Target)
    }
    # tauri.windows.conf.json selects NSIS and disables updater signing artifacts.
    Invoke-Checked 'npm.cmd' @('run', 'tauri', '--', 'build', '--target', $Target)
    $Metadata = & cargo metadata --offline --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not resolve the Cargo output directory.' }
    $TargetDirectory = ($Metadata | ConvertFrom-Json).target_directory
    $Output = Join-Path $TargetDirectory "$Target/release/bundle/nsis"
    $Installers = @(Get-ChildItem -LiteralPath $Output -Filter '*-setup.exe' -File)
    if ($Installers.Count -eq 0) { throw 'Tauri did not produce a Windows installer.' }
    foreach ($Installer in $Installers) {
        $Checksum = (Get-FileHash -Algorithm SHA256 -LiteralPath $Installer.FullName).Hash.ToLowerInvariant()
        Set-Content -LiteralPath ($Installer.FullName + '.sha256') -Value "$Checksum  $($Installer.Name)" -Encoding ASCII
        Write-Host "Test installer: $($Installer.FullName)"
    }
}
finally { Pop-Location }
