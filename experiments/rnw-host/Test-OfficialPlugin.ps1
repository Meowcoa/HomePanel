param(
    [Parameter(Mandatory)][string]$HostExe,
    [Parameter(Mandatory)][string]$DebugHomePanelExe,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)
$ErrorActionPreference = 'Stop'
$taskHost=[IO.Path]::GetFullPath($HostExe)
$taskParent=[IO.Path]::GetFullPath($DebugHomePanelExe)
$taskEvidence=[IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $taskEvidence) {throw 'Use a new evidence directory.'}
New-Item -ItemType Directory -Path $taskEvidence | Out-Null
$taskHostDir=Split-Path -Parent $taskHost
$taskArtifacts=@($taskHost,$taskParent,(Join-Path $taskHostDir 'Bundle\index.windows.bundle')) | ForEach-Object {
    $taskFile=Get-Item -LiteralPath $_
    [pscustomobject]@{name=$taskFile.Name;bytes=$taskFile.Length;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash}
}
$taskArtifacts | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskEvidence 'artifacts.json') -Encoding utf8NoBOM
$taskReport=Join-Path $taskHostDir 'host-self-test.json'
$taskDiagnostic=Join-Path $taskHostDir 'official-plugin-diagnostic.json'
$taskPayloads=@(
    @{name='sdk.bundle';sha256='aefa10092eb349175a21ea4025f2f8352a4c4461ed05324b4cfb1a2b2b598c01'},
    @{name='switch.bundle';sha256='38f58139ff556c9f0e576962a7e51f885cf508da626e939d9516344d014aa11f'}
)
foreach ($taskPayload in $taskPayloads) {
    $taskFile=Join-Path $taskHostDir ('LocalPlugin\'+$taskPayload.name)
    if ((Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash -ne $taskPayload.sha256) {throw 'Local official source differs from the inspected candidate.'}
}
$taskPayloads | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskEvidence 'payloads.json') -Encoding utf8NoBOM
foreach ($taskExisting in @($taskReport,$taskDiagnostic)) {
    if (Test-Path -LiteralPath $taskExisting) {Copy-Item -LiteralPath $taskExisting -Destination (Join-Path $taskEvidence ('previous-'+[IO.Path]::GetFileName($taskExisting)))}
}
$taskOldAppData=$env:APPDATA
$taskOldLocalAppData=$env:LOCALAPPDATA
$taskProcess=$null
try {
    $env:APPDATA=Join-Path $taskEvidence 'config'
    $env:LOCALAPPDATA=Join-Path $taskEvidence 'cache'
    $taskStart=[DateTime]::UtcNow
    $taskProcess=Start-Process -FilePath $taskParent -ArgumentList @('--official-plugin-host-self-test',('"'+$taskHost+'"')) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskEvidence 'stdout.log') -RedirectStandardError (Join-Path $taskEvidence 'stderr.log')
    if (-not $taskProcess.WaitForExit(45000)) {throw 'Official plugin test timed out.'}
    if ((Get-Item -LiteralPath $taskReport).LastWriteTimeUtc -lt $taskStart) {throw 'No fresh native report.'}
    Copy-Item -LiteralPath $taskReport -Destination (Join-Path $taskEvidence 'results.json')
    if ((Test-Path -LiteralPath $taskDiagnostic) -and (Get-Item -LiteralPath $taskDiagnostic).LastWriteTimeUtc -ge $taskStart) {
        Copy-Item -LiteralPath $taskDiagnostic -Destination (Join-Path $taskEvidence 'diagnostic.json')
        Get-Content -LiteralPath (Join-Path $taskEvidence 'diagnostic.json') -Encoding utf8
    }
    $taskResult=Get-Content -LiteralPath $taskReport -Encoding utf8 -Raw | ConvertFrom-Json
    Get-Content -LiteralPath $taskReport -Encoding utf8
    if ($taskProcess.ExitCode -ne 0 -or -not $taskResult.passed -or -not $taskResult.officialPluginLoaded -or -not $taskResult.officialPluginMounted -or -not $taskResult.officialPluginStable -or -not $taskResult.brokerRoundTrip -or $taskResult.officialPluginFailed -or $taskResult.realDeviceAccess) {
        throw "Official plugin runtime check did not pass. Evidence: $taskEvidence"
    }
    $taskFreshDiagnostic=Join-Path $taskEvidence 'diagnostic.json'
    if (-not (Test-Path -LiteralPath $taskFreshDiagnostic)) {throw 'Official plugin diagnostic is missing.'}
    $taskDiagnosticResult=Get-Content -LiteralPath $taskFreshDiagnostic -Encoding utf8 -Raw | ConvertFrom-Json
    if ($taskDiagnosticResult.phase -match 'error|failed') {throw "Plugin diagnostic reports failure: $($taskDiagnosticResult.phase)"}
    foreach ($taskBase in @($env:APPDATA,$env:LOCALAPPDATA)) {
        if (Test-Path -LiteralPath (Join-Path $taskBase 'io.github.meowcoa.homepanel')) {throw 'Test initialized product account storage.'}
    }
} finally {
    $env:APPDATA=$taskOldAppData
    $env:LOCALAPPDATA=$taskOldLocalAppData
    if ($null -ne $taskProcess -and -not $taskProcess.HasExited) {
        $taskChildren=Get-CimInstance Win32_Process -Filter "ParentProcessId=$($taskProcess.Id)"
        foreach ($taskChild in $taskChildren) {
            if ($taskChild.ExecutablePath -eq $taskHost) {
                $taskWindow=Get-Process -Id $taskChild.ProcessId
                $null=$taskWindow.CloseMainWindow()
                if (-not $taskWindow.WaitForExit(3000)) {$taskWindow.Kill()}
            }
        }
        if (-not $taskProcess.WaitForExit(3000)) {
            if ($taskProcess.Path -ne $taskParent) {throw 'Test parent identity changed.'}
            $taskProcess.Kill()
        }
    }
}
