param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][ValidateSet('x64','arm64')][string]$Architecture,
    [switch]$RequireSignature
)
$ErrorActionPreference = 'Stop'
$file = Get-Item -LiteralPath $Executable
if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'Expected a regular Windows executable'
}
$expectedVersion = $Version -replace '^v',''
$info = $file.VersionInfo
if ($info.ProductName -cne 'Fleab' -or $info.OriginalFilename -cne 'fleab.exe' -or
    $info.FileVersion -cne $expectedVersion -or $info.ProductVersion -cne $expectedVersion) {
    throw 'Windows executable metadata does not match the Fleab release'
}
$stream = [IO.File]::OpenRead($file.FullName)
$reader = [IO.BinaryReader]::new($stream)
try {
    if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw 'Invalid DOS header' }
    $stream.Position = 0x3c
    $offset = $reader.ReadUInt32()
    if ($offset -gt $stream.Length - 6) { throw 'Invalid PE header offset' }
    $stream.Position = $offset
    if ($reader.ReadUInt32() -ne 0x4550) { throw 'Invalid PE signature' }
    $machine = $reader.ReadUInt16()
    $expectedMachine = if ($Architecture -eq 'x64') { 0x8664 } else { 0xaa64 }
    if ($machine -ne $expectedMachine) { throw 'Windows executable has the wrong CPU architecture' }
} finally { $reader.Dispose(); $stream.Dispose() }
if ($RequireSignature) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid' -or !$signature.SignerCertificate -or
        $signature.SignatureType -ne 'Authenticode' -or !$signature.TimeStamperCertificate) {
        throw "Expected a trusted, timestamped Authenticode signature; received $($signature.Status)"
    }
}
Write-Output "Verified Fleab $expectedVersion Windows $Architecture$(if ($RequireSignature) { ' signature' })"
