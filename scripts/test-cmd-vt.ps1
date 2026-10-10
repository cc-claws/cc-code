param(
    [Parameter(Mandatory = $true)]
    [string]$TestBinary,
    [string]$EvidencePath = (Join-Path ([System.IO.Path]::GetTempPath()) ('cc-code-cmd-vt-' + [guid]::NewGuid().ToString('N') + '.json'))
)

$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path -LiteralPath $TestBinary).Path
$resultPath = [System.IO.Path]::GetFullPath($EvidencePath)
if (Test-Path -LiteralPath $resultPath) {
    throw "Evidence file already exists: $resultPath"
}
$originalFlag = $env:PERI_ISOLATED_CONSOLE_TEST
$originalResult = $env:PERI_CONSOLE_TEST_RESULT
try {
    $env:PERI_ISOLATED_CONSOLE_TEST = '1'
    $env:PERI_CONSOLE_TEST_RESULT = $resultPath
    # New hidden console: never change the user's active CMD window or call model APIs.
    $process = Start-Process -FilePath $binaryPath -WindowStyle Hidden -PassThru -ArgumentList @(
        '--ignored', '--exact', 'conpty::vt_tests::test_windows_console_vt_recovery', '--test-threads=1'
    )
    if (-not $process.WaitForExit(60000)) {
        Stop-Process -Id $process.Id -Force
        throw 'Isolated VT recovery test timed out.'
    }
    if ($process.ExitCode -ne 0) {
        throw "Console VT recovery failed (exit $($process.ExitCode)). Partial evidence: $resultPath"
    }
    $cases = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($cases.Count -ne 8) {
        throw "Expected eight mode/code-page cases, got $($cases.Count)."
    }
    Write-Output "Passed 8 real console VT recovery cases (192 frames). Evidence: $resultPath"
}
finally {
    $env:PERI_ISOLATED_CONSOLE_TEST = $originalFlag
    $env:PERI_CONSOLE_TEST_RESULT = $originalResult
}
