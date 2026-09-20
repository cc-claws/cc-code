param(
    [Parameter(Mandatory = $true)]
    [string]$TestBinary,
    [string]$EvidencePath = (Join-Path ([System.IO.Path]::GetTempPath()) ('peri-cmd-render-' + [guid]::NewGuid().ToString('N') + '.json'))
)

$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path -LiteralPath $TestBinary).Path
$resultPath = [System.IO.Path]::GetFullPath($EvidencePath)
if (Test-Path -LiteralPath $resultPath) {
    throw "Evidence file already exists; choose a new path: $resultPath"
}
$originalFlag = $env:PERI_ISOLATED_CONSOLE_TEST
$originalResult = $env:PERI_CONSOLE_TEST_RESULT
try {
    $env:PERI_ISOLATED_CONSOLE_TEST = '1'
    $env:PERI_CONSOLE_TEST_RESULT = $resultPath
    # Start-Process creates a separate hidden console; the test changes only that console's font.
    $process = Start-Process -FilePath $binaryPath -WindowStyle Hidden -PassThru -Wait -ArgumentList @(
        '--ignored',
        '--exact',
        'terminal_backend::windows::tests::test_windows_backend_real_console_ghosting',
        '--test-threads=1'
    )
    if ($process.ExitCode -ne 0) {
        throw "Console rendering regression failed (exit $($process.ExitCode)). Partial evidence: $resultPath"
    }
    if (-not (Test-Path -LiteralPath $resultPath)) {
        throw 'The isolated test did not produce evidence; check the selected test binary.'
    }
    $cases = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($cases.Count -eq 1 -and $cases[0] -is [array]) {
        $cases = $cases[0]
    }
    if ($cases.Count -ne 4) {
        throw "Expected four font/code-page cases, got $($cases.Count)."
    }
    Write-Output "Passed 4 console rendering cases. Evidence: $resultPath"
}
finally {
    $env:PERI_ISOLATED_CONSOLE_TEST = $originalFlag
    $env:PERI_CONSOLE_TEST_RESULT = $originalResult
}
