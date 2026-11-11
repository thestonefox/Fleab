# Runs only INSIDE Windows Sandbox, never on the development computer.
param([int]$TestUpdater = 0)
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
    if ($TestUpdater) {
        $updateTest = Start-Process C:\FleabInput\updater_acceptance.exe -ArgumentList '--ignored','--nocapture' -PassThru -RedirectStandardOutput C:\FleabResults\updater-tests.txt -RedirectStandardError C:\FleabResults\updater-errors.txt
        # Retain the native handle before exit; Windows PowerShell otherwise
        # sometimes returns a null ExitCode for short-lived redirected children.
        $updateHandle = $updateTest.Handle
        if (!$updateTest.WaitForExit(180000)) { throw 'Desktop update/recovery acceptance timed out' }
        if ($updateTest.ExitCode -ne 0) { throw 'Desktop update/recovery acceptance failed' }
        $result.details += 'Signed update restarted; failed startup restored original app; profiles and games preserved'
    }
    $result.passed = $true
} catch { $result.error = $_.Exception.ToString() }
finally {
    $result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath C:\FleabResults\result.json
    Stop-Transcript
    # Only the disposable guest shuts down. The guard above prevents host use.
    & C:\Windows\System32\shutdown.exe /s /t 0
}
