param(
    [Parameter(Mandatory = $true)]
    [string]$TestBinary,
    [string]$EvidencePath = (Join-Path ([System.IO.Path]::GetTempPath()) ('peri-shell-console-' + [guid]::NewGuid().ToString('N') + '.json'))
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
    # Create a separate hidden console; only that console's code page is changed.
    $process = Start-Process -FilePath $binaryPath -WindowStyle Hidden -PassThru -ArgumentList @(
        '--ignored', '--exact',
        'terminal_backend::windows::shell_console_test::test_shell_console_php_isolation',
        '--test-threads=1'
    )
    if (-not $process.WaitForExit(60000)) {
        Stop-Process -Id $process.Id -Force
        throw 'Isolated console test timed out.'
    }
    if ($process.ExitCode -ne 0) {
        throw "Console test failed (exit $($process.ExitCode)). Partial evidence: $resultPath"
    }
    $cases = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($cases.Count -ne 8) {
        throw "Expected eight shell/executor cases, got $($cases.Count)."
    }
    Write-Output "Passed 8 real console cases. Evidence: $resultPath"
}
finally {
    $env:PERI_ISOLATED_CONSOLE_TEST = $originalFlag
    $env:PERI_CONSOLE_TEST_RESULT = $originalResult
}
