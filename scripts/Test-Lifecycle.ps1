param([Parameter(Mandatory)][string]$WorkspaceRoot)

$ErrorActionPreference = 'Stop'
$taskWorkspace = [IO.Path]::GetFullPath($WorkspaceRoot)
$taskExe = [IO.Path]::GetFullPath((Join-Path $taskWorkspace 'work\cargo-target\debug\homepanel.exe'))
if (-not (Test-Path -LiteralPath $taskExe -PathType Leaf)) { throw 'Build the Debug binary before running lifecycle tests.' }
if (Get-Process -Name homepanel -ErrorAction SilentlyContinue | Where-Object Path -eq $taskExe) {
    throw 'An existing Debug preview must be preserved. Close it before testing.'
}
$taskQaRoot = Join-Path $taskWorkspace ('work\HomePanel-lifecycle-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskQaRoot | Out-Null
$taskOriginalRoaming = $env:APPDATA
$taskOriginalLocal = $env:LOCALAPPDATA
$taskResults = [Collections.Generic.List[object]]::new()
$taskChild = $null
try {
    foreach ($taskScenario in @('visible-quit','hidden-quit','hidden-open-quit','hidden-relaunch-quit','blocking-quit')) {
        $taskDir = Join-Path $taskQaRoot $taskScenario
        New-Item -ItemType Directory -Path $taskDir,(Join-Path $taskDir 'Roaming'),(Join-Path $taskDir 'Local') | Out-Null
        $env:APPDATA = Join-Path $taskDir 'Roaming'
        $env:LOCALAPPDATA = Join-Path $taskDir 'Local'
        $taskClock = [Diagnostics.Stopwatch]::StartNew()
        $taskChild = Start-Process -FilePath $taskExe -ArgumentList ('--demo-lifecycle=' + $taskScenario) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskDir 'stdout.log') -RedirectStandardError (Join-Path $taskDir 'stderr.log')
        if (-not $taskChild.WaitForExit(6500)) { throw "Lifecycle process remained alive after exit request: $taskScenario" }
        $taskClock.Stop()
        if ($taskChild.ExitCode -ne 0) { throw "Lifecycle scenario failed ($($taskChild.ExitCode)): $taskScenario" }
        $taskTrace = Get-Content -LiteralPath (Join-Path $taskDir 'stderr.log') -Encoding utf8 -Raw
        if ($taskTrace -notmatch 'LIFECYCLE quit-requested') { throw "Quit path was not exercised: $taskScenario" }
        if ($taskScenario.StartsWith('hidden') -and $taskTrace -notmatch 'LIFECYCLE hidden=true') { throw 'Panel was not hidden before the test.' }
        if ($taskScenario -in @('hidden-open-quit','hidden-relaunch-quit') -and $taskTrace -notmatch 'LIFECYCLE reopened=true') { throw 'Hidden panel did not reopen.' }
        if ($taskScenario -eq 'hidden-relaunch-quit' -and $taskTrace -notmatch 'PREVIEW native-restore visible=true') { throw 'Native restore did not synchronize application visibility.' }
        # A clean exit must release the executable too, so an upgrade can write it.
        $taskProbe = [IO.File]::Open($taskExe, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $taskProbe.Dispose()
        $taskResults.Add([ordered]@{scenario=$taskScenario;passed=$true;elapsedMs=$taskClock.ElapsedMilliseconds;exitCode=$taskChild.ExitCode;executableReleased=$true})
        $taskChild = $null
    }
}
finally {
    $env:APPDATA = $taskOriginalRoaming
    $env:LOCALAPPDATA = $taskOriginalLocal
    if ($null -ne $taskChild -and -not $taskChild.HasExited) {
        $taskChild.Refresh()
        if ($taskChild.Path -ne $taskExe) { throw 'Test process identity changed; will not stop it.' }
        Stop-Process -Id $taskChild.Id
        if (-not $taskChild.WaitForExit(2000)) { throw 'Test process did not stop.' }
    }
    $taskResults | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskQaRoot 'results.json') -Encoding utf8NoBOM
}
$taskResults | ConvertTo-Json -Depth 4
"Lifecycle evidence: $taskQaRoot"
