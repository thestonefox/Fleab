param([string]$UnityArchive = '', [string]$UpdaterFixtures = '')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Set-Location $repo
$runId = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$inputDir = Join-Path $repo ('.tools\sandbox-input-' + $runId)
$outputDir = Join-Path $repo ('.tools\sandbox-results-' + $runId)
New-Item -ItemType Directory -Force -Path $inputDir,$outputDir,(Join-Path $inputDir 'fixtures') | Out-Null
Copy-Item -LiteralPath (Join-Path $repo 'target\release\fleab.exe') -Destination (Join-Path $inputDir 'fleab.exe')
$build = & cargo test --locked --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Could not build test executables' }
foreach ($line in $build) {
    try { $item = $line | ConvertFrom-Json } catch { continue }
    if ($item.reason -ne 'compiler-artifact' -or !$item.executable -or !$item.profile.test) { continue }
    if ($item.target.kind -contains 'lib') { $name = 'unit' }
    elseif ($item.target.name -in @('core','launch','acceptance','symlinks','updater_acceptance','tray_acceptance')) { $name = $item.target.name }
    else { continue }
    Copy-Item -LiteralPath $item.executable -Destination (Join-Path $inputDir "$name.exe")
}
Get-ChildItem -LiteralPath (Join-Path $repo 'tests\fixtures') -Filter '*.tar.zst' | Copy-Item -Destination (Join-Path $inputDir 'fixtures')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'sandbox-guest.ps1') -Destination $inputDir
if ($UnityArchive) { Copy-Item -LiteralPath $UnityArchive -Destination $inputDir }
if ($UpdaterFixtures) {
    foreach ($case in @('good','broken')) {
        $destination = Join-Path $inputDir "fixtures\$case"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        foreach ($file in @('fleab-update.json','update.tar.gz')) {
            Copy-Item -LiteralPath (Join-Path $UpdaterFixtures "$case\$file") -Destination $destination
        }
    }
}
$safeInput = [Security.SecurityElement]::Escape($inputDir)
$safeOutput = [Security.SecurityElement]::Escape($outputDir)
$config = @"
<Configuration>
  <VGpu>Enable</VGpu>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MemoryInMB>4096</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$safeInput</HostFolder><SandboxFolder>C:\FleabInput</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$safeOutput</HostFolder><SandboxFolder>C:\FleabResults</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\FleabInput\sandbox-guest.ps1 -TestUpdater $([int][bool]$UpdaterFixtures)</Command></LogonCommand>
</Configuration>
"@
$configPath = Join-Path $outputDir 'Fleab.wsb'
[IO.File]::WriteAllText($configPath,$config)
Start-Process -FilePath 'C:\Windows\System32\WindowsSandbox.exe' -ArgumentList ('"' + $configPath + '"') -WindowStyle Hidden
Write-Output "Sandbox results: $outputDir"
