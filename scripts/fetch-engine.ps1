<#
.SYNOPSIS
  Fetch the SHA-256-pinned Windows x64 CPU engine and its SSE2 fallback.
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/fetch-engine.ps1
#>
[CmdletBinding()]
param(
    [ValidateSet('auto', 'windows')]
    [string]$Platform = 'auto',
    [string]$Dest,
    [string]$Baseline,
    [switch]$AllTools,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Pin = Get-Content -LiteralPath (Join-Path $Root 'engine.lock.json') -Raw | ConvertFrom-Json
$ArchiveSpec = $Pin.archives.'windows-x64'
$Version = $Pin.release
$Repo = $Pin.repository
$File = $ArchiveSpec.file
$ExpectedSha = $ArchiveSpec.sha256
if ($File -notmatch '^[a-zA-Z0-9._-]+\.zip$' -or $ExpectedSha -notmatch '^[a-fA-F0-9]{64}$') {
    throw 'Invalid Windows archive pin in engine.lock.json.'
}
if (-not $Dest) { $Dest = Join-Path $Root 'src-tauri/resources/engine' }
if (-not $Baseline) { $Baseline = Join-Path (Split-Path $Dest -Parent) 'engine-baseline' }
$Dest = [System.IO.Path]::GetFullPath($Dest).TrimEnd([char[]]'\/')
$Baseline = [System.IO.Path]::GetFullPath($Baseline).TrimEnd([char[]]'\/')
# Installation replaces these two directories; neither may contain the other.
$Separator = [System.IO.Path]::DirectorySeparatorChar
if (-not (Split-Path $Dest -Parent) -or -not (Split-Path $Baseline -Parent) -or
    $Dest.Equals($Baseline, [StringComparison]::OrdinalIgnoreCase) -or
    $Dest.StartsWith($Baseline + $Separator, [StringComparison]::OrdinalIgnoreCase) -or
    $Baseline.StartsWith($Dest + $Separator, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Engine and baseline destinations must be separate, non-root directories.'
}
$Required = @('llama-server.exe', 'llama-server-impl.dll', 'llama-common.dll',
    'llama.dll', 'ggml.dll', 'ggml-base.dll', 'mtmd.dll', 'libomp140.x86_64.dll', 'LICENSE')

function Test-Payload([string]$Path, [bool]$BaselineOnly) {
    foreach ($Name in $Required + @('ggml-cpu-x64.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Path $Name) -PathType Leaf)) { return $false }
    }
    $Variants = @(Get-ChildItem -LiteralPath $Path -Filter 'ggml-cpu-*.dll' -File)
    if ($BaselineOnly) { return $Variants.Count -eq 1 }
    return Test-Path -LiteralPath (Join-Path $Path 'ggml-cpu-ssse3.dll') -PathType Leaf
}
function Test-CachedPin([string]$Path) {
    try {
        $Identity = Get-Content -LiteralPath (Join-Path $Path 'engine-install.json') -Raw | ConvertFrom-Json
        return $Identity.archive_sha256 -eq $ExpectedSha -and $Identity.version -eq $Version -and
            $Identity.all_tools -eq [bool]$AllTools -and
            (-not $AllTools -or (Test-Path -LiteralPath (Join-Path $Path 'llama-bench.exe')))
    }
    catch { return $false }
}
if (-not $Force -and (Test-CachedPin $Dest) -and (Test-CachedPin $Baseline) -and
    (Test-Payload $Dest $false) -and (Test-Payload $Baseline $true)) {
    Write-Host "Pinned Windows engine and baseline are already present ($Version)."
    return
}

$Id = [guid]::NewGuid().ToString('N')
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('tnn-engine-' + $Id)
$Stage = $Dest + '.staging-' + $Id
$BaselineStage = $Baseline + '.staging-' + $Id
$Backup = $Dest + '.backup-' + $Id
$BaselineBackup = $Baseline + '.backup-' + $Id
$Committed = $false
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $Archive = Join-Path $Tmp $File
    Write-Host "Downloading $Version (Windows x64)..."
    Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Version/$File" `
        -OutFile $Archive -UseBasicParsing
    $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash.ToLowerInvariant()
    if ($Actual -ne $ExpectedSha) { throw "Checksum mismatch for $File." }
    $Extract = Join-Path $Tmp 'extract'
    Expand-Archive -LiteralPath $Archive -DestinationPath $Extract
    New-Item -ItemType Directory -Path $Stage -Force | Out-Null
    if ($AllTools) {
        Get-ChildItem -LiteralPath $Extract | Copy-Item -Destination $Stage -Recurse
    }
    else {
        foreach ($Name in $Required) {
            $Source = Join-Path $Extract $Name
            if (Test-Path -LiteralPath $Source) { Copy-Item -LiteralPath $Source -Destination $Stage }
        }
        Get-ChildItem -LiteralPath $Extract -Filter 'ggml-cpu-*.dll' -File |
            Copy-Item -Destination $Stage
    }
    if (-not (Test-Path -LiteralPath (Join-Path $Stage 'LICENSE'))) {
        Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$Repo/$Version/LICENSE" `
            -OutFile (Join-Path $Stage 'LICENSE') -UseBasicParsing
    }
    if (-not (Test-Payload $Stage $false)) { throw 'The pinned archive is missing a required engine DLL or CPU variant.' }
    New-Item -ItemType Directory -Path $BaselineStage -Force | Out-Null
    Get-ChildItem -LiteralPath $Stage | Copy-Item -Destination $BaselineStage -Recurse
    Get-ChildItem -LiteralPath $BaselineStage -Filter 'ggml-cpu-*.dll' -File |
        Where-Object { $_.Name -ne 'ggml-cpu-x64.dll' } | Remove-Item -Force
    if (-not (Test-Payload $BaselineStage $true)) { throw 'The SSE2 baseline payload is incomplete.' }
    $Identity = @{ version = $Version; archive_sha256 = $ExpectedSha; all_tools = [bool]$AllTools } | ConvertTo-Json
    foreach ($Path in @($Stage, $BaselineStage)) {
        Set-Content -LiteralPath (Join-Path $Path 'engine-install.json') -Value $Identity -Encoding UTF8
    }
    foreach ($Pair in @(@($Dest, $Stage), @($Baseline, $BaselineStage))) {
        $Keep = Join-Path $Pair[0] '.gitkeep'
        if (Test-Path -LiteralPath $Keep) { Copy-Item -LiteralPath $Keep -Destination $Pair[1] }
    }
    # Stages/backups are siblings of their destinations, including on another drive.
    $Placed = $false
    $BaselinePlaced = $false
    try {
        if (Test-Path -LiteralPath $Dest) { Move-Item -LiteralPath $Dest -Destination $Backup }
        if (Test-Path -LiteralPath $Baseline) { Move-Item -LiteralPath $Baseline -Destination $BaselineBackup }
        Move-Item -LiteralPath $Stage -Destination $Dest
        $Placed = $true
        Move-Item -LiteralPath $BaselineStage -Destination $Baseline
        $BaselinePlaced = $true
        $Committed = $true
    }
    finally {
        if (-not $Committed) {
            if ($Placed) { Remove-Item -LiteralPath $Dest -Recurse -Force }
            if ($BaselinePlaced) { Remove-Item -LiteralPath $Baseline -Recurse -Force }
            if (Test-Path -LiteralPath $Backup) { Move-Item -LiteralPath $Backup -Destination $Dest }
            if (Test-Path -LiteralPath $BaselineBackup) { Move-Item -LiteralPath $BaselineBackup -Destination $Baseline }
        }
    }
    Write-Host "Installed pinned Windows engine and SSE2 baseline ($Version)."
}
finally {
    foreach ($Path in @($Tmp, $Stage, $BaselineStage)) {
        if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
    }
    if ($Committed) {
        foreach ($Path in @($Backup, $BaselineBackup)) {
            if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
        }
    }
}
