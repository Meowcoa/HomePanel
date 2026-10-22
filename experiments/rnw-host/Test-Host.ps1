param(
    [Parameter(Mandatory)][string]$HostExe,
    [Parameter(Mandatory)][string]$DebugHomePanelExe,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)
$ErrorActionPreference = 'Stop'
$taskHost = [IO.Path]::GetFullPath($HostExe)
$taskParent = [IO.Path]::GetFullPath($DebugHomePanelExe)
$taskEvidence = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $taskEvidence) {throw 'Use a new evidence directory; previous reports are preserved.'}
New-Item -ItemType Directory -Path $taskEvidence | Out-Null
$taskReport = Join-Path (Split-Path -Parent $taskHost) 'host-self-test.json'
if (Test-Path -LiteralPath $taskReport) {
    Copy-Item -LiteralPath $taskReport -Destination (Join-Path $taskEvidence 'previous-host-report.json')
}
$taskOldAppData=$env:APPDATA
$taskOldLocalAppData=$env:LOCALAPPDATA
try {
    $env:APPDATA=Join-Path $taskEvidence 'config'
    $env:LOCALAPPDATA=Join-Path $taskEvidence 'cache'
    $taskStart=[DateTime]::UtcNow
    $taskProcess=Start-Process -FilePath $taskParent -ArgumentList @('--plugin-host-self-test', ('"'+$taskHost+'"')) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskEvidence 'stdout.log') -RedirectStandardError (Join-Path $taskEvidence 'stderr.log')
    if (-not $taskProcess.WaitForExit(25000)) {throw "Broker test timed out: PID $($taskProcess.Id)"}
    if ($taskProcess.ExitCode -ne 0) {throw "Broker test failed: $($taskProcess.ExitCode)"}
    if ((Get-Item -LiteralPath $taskReport).LastWriteTimeUtc -lt $taskStart) {throw 'Host did not generate a fresh report'}
    $taskResult=Get-Content -LiteralPath $taskReport -Encoding utf8 -Raw | ConvertFrom-Json
    foreach ($taskKey in @('passed','nativeWindow','nativeModule','promiseRoundTrip','nativeViewLayout','nativeEventRoundTrip','brokerRoundTrip')) {
        if ($taskResult.$taskKey -ne $true) {throw "Host check failed: $taskKey"}
    }
    if ($taskResult.realDeviceAccess -ne $false -or $taskResult.officialPluginLoaded -ne $false) {throw 'Self-test scope changed'}
    # Windows itself may create an empty Microsoft/Windows/Caches directory.
    # The product's account/config/cache directories must remain absent.
    foreach ($taskBase in @($env:APPDATA,$env:LOCALAPPDATA)) {
        foreach ($taskId in @('io.github.meowcoa.homepanel','io.github.meowcoa.homepanel.demo')) {
            if (Test-Path -LiteralPath (Join-Path $taskBase $taskId)) {throw 'Self-test initialized product account storage'}
        }
    }
    Copy-Item -LiteralPath $taskReport -Destination (Join-Path $taskEvidence 'results.json')
    Get-Content -LiteralPath (Join-Path $taskEvidence 'results.json') -Encoding utf8
} finally {
    $env:APPDATA=$taskOldAppData
    $env:LOCALAPPDATA=$taskOldLocalAppData
}
