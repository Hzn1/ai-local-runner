# Local fixtures: no model files, network, Windows host or Pester dependency.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Source = Join-Path $PSScriptRoot '../fetch-engine.ps1'
$Root = Join-Path ([IO.Path]::GetTempPath()) ('tnn windows fixture ' + [char]0x00E7 + [char]0x00E3 + ' ' + [guid]::NewGuid().ToString('N'))
$ScriptRoot = Join-Path $Root 'scripts'
$Payload = Join-Path $Root 'payload'
$Archive = Join-Path $Root 'fixture.zip'
$Dest = Join-Path $Root 'resources/engine'
$Baseline = Join-Path $Root 'resources/engine-baseline'
$FixtureState = [pscustomobject]@{ Archive = $Archive; Baseline = $Baseline; Downloads = 0; FailBaselineMove = $false }
function Assert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
Set-Item -Path Function:Invoke-WebRequest -Value ({
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    if ($Uri.EndsWith('/LICENSE')) { Set-Content -LiteralPath $OutFile -Value 'MIT fixture' }
    else { Copy-Item -LiteralPath $FixtureState.Archive -Destination $OutFile; $FixtureState.Downloads++ }
}.GetNewClosure())
Set-Item -Path Function:Move-Item -Value ({
    param($LiteralPath, $Destination)
    if ($FixtureState.FailBaselineMove -and $LiteralPath -like '*.staging-*' -and $Destination -eq $FixtureState.Baseline) {
        $FixtureState.FailBaselineMove = $false
        throw 'Injected second-directory install failure'
    }
    Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
}.GetNewClosure())
function Write-FixturePin {
    if (Test-Path -LiteralPath $Archive) { Remove-Item -LiteralPath $Archive }
    Compress-Archive -Path (Join-Path $Payload '*') -DestinationPath $Archive
    $Hash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    @{ release = 'test-pin'; repository = 'example/fixture'; archives = @{
        'windows-x64' = @{ file = 'fixture.zip'; sha256 = $Hash }
    }} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Root 'engine.lock.json')
}
try {
    New-Item -ItemType Directory -Path $ScriptRoot, $Payload -Force | Out-Null
    Copy-Item -LiteralPath $Source -Destination $ScriptRoot
    $Fetch = Join-Path $ScriptRoot 'fetch-engine.ps1'
    foreach ($Name in @('llama-server.exe', 'llama-server-impl.dll', 'llama-common.dll',
        'llama.dll', 'ggml.dll', 'ggml-base.dll', 'mtmd.dll', 'libomp140.x86_64.dll',
        'ggml-cpu-x64.dll', 'ggml-cpu-ssse3.dll', 'ggml-cpu-haswell.dll', 'llama-cli.exe', 'llama-bench.exe')) {
        Set-Content -LiteralPath (Join-Path $Payload $Name) -Value 'fixture bytes'
    }
    Write-FixturePin
    & $Fetch -Dest $Dest -Baseline $Baseline
    Assert (Test-Path (Join-Path $Dest 'ggml-cpu-ssse3.dll')) 'Native dispatch DLL was pruned.'
    Assert (-not (Test-Path (Join-Path $Dest 'llama-cli.exe'))) 'Default payload kept unused tools.'
    Assert (@(Get-ChildItem $Baseline -Filter 'ggml-cpu-*.dll').Count -eq 1) 'Baseline includes extra CPU variants.'
    & $Fetch -Dest $Dest -Baseline $Baseline
    Assert ($FixtureState.Downloads -eq 1) 'A complete cached pin was downloaded again.'
    Remove-Item -LiteralPath (Join-Path $Baseline 'ggml-cpu-x64.dll')
    & $Fetch -Dest $Dest -Baseline $Baseline
    Assert ($FixtureState.Downloads -eq 2) 'Missing baseline was not repaired.'
    & $Fetch -Dest $Dest -Baseline $Baseline -AllTools
    Assert (Test-Path (Join-Path $Dest 'llama-bench.exe')) 'AllTools omitted the benchmark executable.'
    & $Fetch -Dest $Dest -Baseline $Baseline
    Assert (-not (Test-Path (Join-Path $Dest 'llama-cli.exe'))) 'AllTools cache leaked into a default installer payload.'
    Set-Content -LiteralPath (Join-Path $Dest 'previous.txt') -Value 'preserve me'
    Set-Content -LiteralPath (Join-Path $Baseline 'previous.txt') -Value 'preserve me'
    # A corrupted archive must leave both existing directories untouched.
    Add-Content -LiteralPath $Archive -Value 'corrupt checksum'
    $Failed = $false
    try { & $Fetch -Dest $Dest -Baseline $Baseline -Force } catch { $Failed = $true }
    Assert $Failed 'A bad checksum was accepted.'
    Assert (Test-Path (Join-Path $Dest 'previous.txt')) 'Checksum failure replaced the existing engine.'
    Remove-Item -LiteralPath (Join-Path $Payload 'ggml-base.dll')
    Write-FixturePin
    $Failed = $false
    try { & $Fetch -Dest $Dest -Baseline $Baseline -Force } catch { $Failed = $true }
    Assert $Failed 'An incomplete DLL payload was accepted.'
    Assert (Test-Path (Join-Path $Dest 'previous.txt')) 'Incomplete payload replaced the existing engine.'
    Set-Content -LiteralPath (Join-Path $Payload 'ggml-base.dll') -Value 'fixture bytes'
    Write-FixturePin
    $FixtureState.FailBaselineMove = $true
    $Failed = $false
    try { & $Fetch -Dest $Dest -Baseline $Baseline -Force } catch { $Failed = $true }
    Assert $Failed 'Injected install failure was not reported.'
    Assert (Test-Path (Join-Path $Dest 'previous.txt')) 'Engine was not rolled back.'
    Assert (Test-Path (Join-Path $Baseline 'previous.txt')) 'Baseline was not rolled back.'
    Assert (@(Get-ChildItem (Split-Path $Dest -Parent) -Filter '*.staging-*').Count -eq 0) 'Staging directories leaked.'
    $Failed = $false
    try { & $Fetch -Dest $Dest -Baseline (Join-Path $Dest 'nested') -Force } catch { $Failed = $true }
    Assert $Failed 'Overlapping destinations were accepted.'
    $Tokens = $null
    $ParseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot '../build-windows.ps1'), [ref]$Tokens, [ref]$ParseErrors)
    Assert ($ParseErrors.Count -eq 0) 'Windows build script has syntax errors.'
    Write-Host 'PASS: pinned fetch, cache repair, AllTools pruning, checksum/DLL failure, rollback, Unicode paths and build-script parsing.'
}
finally { if (Test-Path -LiteralPath $Root) { Remove-Item -LiteralPath $Root -Recurse -Force } }
