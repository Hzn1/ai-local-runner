<#
.SYNOPSIS
  Fetch and prune the pinned PrismML llama.cpp Windows CPU build.

.DESCRIPTION
  Values mirror engine.lock.json. The archive is verified against its pinned
  SHA-256 before anything is installed. Run from anywhere; paths default to
  this repository's src-tauri/resources tree.

.EXAMPLE
  pwsh -File scripts/fetch-engine.ps1

.EXAMPLE
  pwsh -File scripts/fetch-engine.ps1 -Force -AllTools
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

$Version = 'prism-b10826-c9c34bc'
$Repo = 'PrismML-Eng/llama.cpp'
$File = 'llama-prism-b10826-c9c34bc-bin-win-cpu-x64.zip'
$ExpectedSha = '1e88ab9e858b5c2dbc30cf847947a9f923b53511ba0baf0b265353ebe1f4daa6'

if ($Platform -ne 'windows' -and $Platform -ne 'auto') {
    throw 'This script installs the Windows build. Use scripts/fetch-engine.sh on Linux.'
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Dest) { $Dest = Join-Path $Root 'src-tauri/resources/engine' }
if (-not $Baseline) { $Baseline = Join-Path (Split-Path $Dest -Parent) 'engine-baseline' }
$Dest = [System.IO.Path]::GetFullPath($Dest)
$Baseline = [System.IO.Path]::GetFullPath($Baseline)

$exe = Join-Path $Dest 'llama-server.exe'
if ((Test-Path $exe) -and -not $Force) {
    Write-Host "Engine already present in $Dest (use -Force to replace)."
    exit 0
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tnn-engine-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $archive = Join-Path $tmp $File
    $url = "https://github.com/$Repo/releases/download/$Version/$File"
    Write-Host "Downloading $Version (windows)..."
    Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing

    $actual = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
    if ($actual -ne $ExpectedSha) {
        throw "Checksum mismatch for $File`n  expected: $ExpectedSha`n  actual:   $actual"
    }

    $extract = Join-Path $tmp 'extract'
    Expand-Archive -Path $archive -DestinationPath $extract -Force

    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    New-Item -ItemType Directory -Path $Dest | Out-Null

    if ($AllTools) {
        Copy-Item -Path (Join-Path $extract '*') -Destination $Dest -Recurse -Force
    }
    else {
        $keep = @(
            'llama-server.exe',
            'llama-server-impl.dll',
            'llama-common.dll',
            'llama.dll',
            'ggml.dll',
            'ggml-base.dll',
            'mtmd.dll',
            'libomp140.x86_64.dll',
            'LICENSE'
        )
        foreach ($name in $keep) {
            $source = Join-Path $extract $name
            if (Test-Path $source) { Copy-Item $source (Join-Path $Dest $name) }
        }
        Get-ChildItem -Path $extract -Filter 'ggml-cpu-*.dll' -File |
            ForEach-Object { Copy-Item $_.FullName (Join-Path $Dest $_.Name) }
    }

    if (-not (Test-Path (Join-Path $Dest 'LICENSE'))) {
        Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$Repo/$Version/LICENSE" `
            -OutFile (Join-Path $Dest 'LICENSE') -UseBasicParsing
    }

    $baselineVariant = Join-Path $Dest 'ggml-cpu-x64.dll'
    if (-not (Test-Path $baselineVariant)) {
        throw 'The archive no longer contains the x64 baseline CPU variant.'
    }
    if (-not (Test-Path $exe)) {
        throw 'The archive no longer contains llama-server.exe.'
    }

    if (Test-Path $Baseline) { Remove-Item -Recurse -Force $Baseline }
    New-Item -ItemType Directory -Path $Baseline | Out-Null
    Copy-Item -Path (Join-Path $Dest '*') -Destination $Baseline -Recurse -Force
    Get-ChildItem -Path $Baseline -Filter 'ggml-cpu-*.dll' -File |
        Where-Object { $_.Name -ne 'ggml-cpu-x64.dll' } |
        Remove-Item -Force

    Write-Host "Engine installed: $Dest"
    Write-Host "Baseline copy:    $Baseline"
    Write-Host "Version:          $Version"
}
finally {
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
}
