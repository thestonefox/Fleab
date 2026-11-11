param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][ValidateSet('x64','arm64')][string]$Architecture
)
$ErrorActionPreference = 'Stop'
$verify = Join-Path $PSScriptRoot '../../tools/verify-windows-release.ps1'
& $verify -Executable $Executable -Version $Version -Architecture $Architecture
$other = if ($Architecture -eq 'x64') { 'arm64' } else { 'x64' }
$cases = @(
    @{Version='v99999.0.0'; Architecture=$Architecture},
    @{Version=$Version; Architecture=$other},
    @{Version=$Version; Architecture=$Architecture; RequireSignature=$true}
)
foreach ($case in $cases) {
    $rejected = $false
    try { & $verify -Executable $Executable @case }
    catch { $rejected = $true }
    if (!$rejected) { throw 'An invalid or unsigned release passed publisher verification' }
}
Write-Output 'Windows version, architecture and unsigned-release rejection checks passed'
