# Runs only INSIDE Windows Sandbox, never on the development computer.
$ErrorActionPreference = 'Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'This script must run inside Windows Sandbox.' }
$result = @{ passed = $false; details = @() }
New-Item -ItemType Directory -Force -Path C:\FleabWork | Out-Null
Set-Location C:\FleabWork
Start-Transcript -Path C:\FleabResults\transcript.txt -Force
try {
    $env:FLEAB_FIXTURES_DIR = 'C:\FleabInput\fixtures'
    foreach ($test in @('unit','core','launch','symlinks')) {
        & "C:\FleabInput\$test.exe" --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw "$test tests failed: $LASTEXITCODE" }
        $result.details += "$test tests passed"
    }
    $archive = Get-ChildItem -LiteralPath C:\FleabInput -Filter '*_Windows64.tar.zst' | Select-Object -First 1
    if ($archive) {
        $env:FLEAB_ACCEPTANCE_ARCHIVE = $archive.FullName
        & C:\FleabInput\acceptance.exe real_unity_player --ignored --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Real Unity player acceptance failed' }
        $result.details += 'Real Unity archive deployed and launched; running process detected'
    }
    $app = Start-Process -FilePath C:\FleabInput\fleab.exe -PassThru
    Start-Sleep -Seconds 5
    $app.Refresh()
    if ($app.HasExited) { throw "Portable GUI exited unexpectedly: $($app.ExitCode)" }
    $result.details += 'Portable GUI started without installed Rust or .NET'
    Stop-Process -Id $app.Id
    $result.passed = $true
} catch { $result.error = $_.Exception.ToString() }
finally {
    $result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath C:\FleabResults\result.json
    Stop-Transcript
    # Only the disposable guest shuts down. The guard above prevents host use.
    & C:\Windows\System32\shutdown.exe /s /t 0
}
