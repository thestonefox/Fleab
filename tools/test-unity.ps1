param([Parameter(Mandatory=$true)][string]$UnityEditor, [switch]$BuildPlayers, [ValidateSet('Windows64','Linux64','macOS')][string]$BuildTarget = 'Windows64')
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $UnityEditor -PathType Leaf)) { throw 'Set UnityEditor to the installed Unity.exe path.' }
$repo = Split-Path $PSScriptRoot -Parent
$root = Join-Path $repo '.tools\unity-validation'
$project = Join-Path $root 'project'
New-Item -ItemType Directory -Force -Path (Join-Path $project 'Assets'),(Join-Path $project 'Packages'),(Join-Path $project 'ProjectSettings') | Out-Null
$package = 'file:' + ((Join-Path $repo 'unity-package') -replace '\\','/')
@{ dependencies = @{ 'com.fleab.publisher' = $package; 'com.unity.test-framework' = '1.6.0' }; testables = @('com.fleab.publisher') } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $project 'Packages\manifest.json')
$env:FLEAB_BUILD_TESTS = if ($BuildPlayers) { '1' } else { '0' }
$env:FLEAB_BUILD_TARGET = $BuildTarget
$target = switch ($BuildTarget) { Windows64 { 'Win64' } Linux64 { 'Linux64' } macOS { 'OSXUniversal' } }
$argsList = @('-batchmode','-nographics','-buildTarget',$target,'-projectPath',('"'+$project+'"'),'-runTests','-testPlatform','EditMode','-testResults',('"'+(Join-Path $root 'results.xml')+'"'),'-logFile',('"'+(Join-Path $root 'results.log')+'"'))
$process = Start-Process -FilePath $UnityEditor -ArgumentList $argsList -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0) { throw "Unity tests failed ($($process.ExitCode)); see $root" }
[xml]$results = Get-Content -LiteralPath (Join-Path $root 'results.xml')
if ($results.'test-run'.failed -ne '0') { throw 'Unity reported test failures.' }
Write-Output "Unity: $($results.'test-run'.passed) passed; $($results.'test-run'.skipped) skipped. Results: $root"
