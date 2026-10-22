param(
    [Parameter(Mandatory)][string]$InstallerPath,
    [Parameter(Mandatory)][string]$ReleaseExe,
    [Parameter(Mandatory)][string]$WorkspaceRoot,
    [switch]$SkipTrayLifecycle,
    [string]$PluginHostDirectory
)

$ErrorActionPreference = 'Stop'
$taskWorkspace = [IO.Path]::GetFullPath($WorkspaceRoot)
$taskWork = [IO.Path]::GetFullPath((Join-Path $taskWorkspace 'work'))
$taskInstaller = [IO.Path]::GetFullPath($InstallerPath)
$taskReleaseExe = [IO.Path]::GetFullPath($ReleaseExe)
if (-not $taskInstaller.StartsWith($taskWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not $taskInstaller.EndsWith('-test.exe', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Only an isolated -test installer inside workspace/work may be tested.'
}
$taskRegistry = 'HKCU:\Software\HomePanelInstallerTest'
if (Test-Path -LiteralPath $taskRegistry) { throw 'An existing installer test registration must be preserved. Use a clean test environment.' }

$taskRunRoot = Join-Path $taskWork ('HomePanel-installer-regression-' + [Guid]::NewGuid().ToString('N'))
$taskInstallDir = [IO.Path]::GetFullPath((Join-Path $taskRunRoot '安装测试 with spaces'))
if (-not $taskInstallDir.StartsWith($taskWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Test install directory is outside workspace/work.'
}
New-Item -ItemType Directory -Path $taskInstallDir | Out-Null
$taskApp = Join-Path $taskInstallDir 'HomePanel.exe'
$taskFixtureSource = Join-Path $taskRunRoot 'lock_fixture.rs'
$taskFixtureExe = Join-Path $taskRunRoot 'lock_fixture.exe'
@'
#![windows_subsystem = "windows"]
fn main() { std::thread::sleep(std::time::Duration::from_secs(120)); }
'@ | Set-Content -LiteralPath $taskFixtureSource -Encoding utf8NoBOM
$taskRustc = if ($env:RUSTC) { $env:RUSTC } else { (Get-Command rustc -ErrorAction Stop).Source }
& $taskRustc --edition=2021 --crate-name installer_lock_fixture $taskFixtureSource -o $taskFixtureExe
if ($LASTEXITCODE -ne 0) { throw "Lock fixture compilation failed: $LASTEXITCODE" }
Copy-Item -LiteralPath $taskFixtureExe -Destination $taskApp
$taskOldLicense = Join-Path $taskInstallDir 'LICENSE'
'old-install-sentinel' | Set-Content -LiteralPath $taskOldLicense -Encoding utf8NoBOM
$taskSentinel = Join-Path $taskInstallDir 'user-sentinel.txt'
'preserve-unrecognized-user-file' | Set-Content -LiteralPath $taskSentinel -Encoding utf8NoBOM
$taskResults = [Collections.Generic.List[object]]::new()
$taskFixture = $null
$taskForeign = $null
$taskForeignDir = Join-Path $taskRunRoot 'other-install'
New-Item -ItemType Directory -Path $taskForeignDir | Out-Null
$taskForeignApp = Join-Path $taskForeignDir 'HomePanel.exe'
Copy-Item -LiteralPath $taskFixtureExe -Destination $taskForeignApp
$taskLifecycleExe = Join-Path $taskWorkspace 'work\cargo-target\debug\homepanel.exe'
if (-not $SkipTrayLifecycle -and -not (Test-Path -LiteralPath $taskLifecycleExe)) { throw 'Build the current Debug app for isolated tray upgrade checks.' }

function Invoke-TestProcess([string]$File, [string[]]$Arguments, [int]$ExpectedExit) {
    $taskChild = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    if (-not $taskChild.WaitForExit(30000)) { throw "Test process timed out: $File (PID $($taskChild.Id))" }
    if ($taskChild.ExitCode -ne $ExpectedExit) { throw "Unexpected exit code $($taskChild.ExitCode), expected $ExpectedExit for $File" }
}

function Stop-TestFixture {
    if ($null -ne $script:taskFixture) {
        $taskRunning = Get-Process -Id $script:taskFixture.Id -ErrorAction SilentlyContinue
        if ($taskRunning) {
            if ($taskRunning.Path -ne $script:taskApp) { throw 'Fixture process identity changed; will not stop it.' }
            Stop-Process -Id $taskRunning.Id
            $taskRunning.WaitForExit()
        }
        $script:taskFixture = $null
    }
}

function Start-IsolatedTray {
    $taskSavedRoaming=$env:APPDATA;$taskSavedLocal=$env:LOCALAPPDATA
    $taskTrace=Join-Path $taskRunRoot ('tray-'+[Guid]::NewGuid().ToString('N')+'.log')
    try {
        $env:APPDATA=Join-Path $taskRunRoot 'tray-config';$env:LOCALAPPDATA=Join-Path $taskRunRoot 'tray-local'
        $taskChild=Start-Process -FilePath $script:taskApp -ArgumentList @('--demo','--background') -WindowStyle Hidden -PassThru -RedirectStandardError $taskTrace
    } finally {$env:APPDATA=$taskSavedRoaming;$env:LOCALAPPDATA=$taskSavedLocal}
    $script:taskFixture=$taskChild
    $taskDeadline=[DateTime]::UtcNow.AddSeconds(10)
    while([DateTime]::UtcNow -lt $taskDeadline) {
        if($taskChild.HasExited){throw 'Isolated tray app exited before update listener was ready.'}
        if((Test-Path -LiteralPath $taskTrace) -and (Get-Content -LiteralPath $taskTrace -Encoding utf8 -Raw) -match 'LIFECYCLE update-listener-ready'){return}
        Start-Sleep -Milliseconds 30
    }
    throw 'Isolated tray listener did not become ready.'
}

try {
    $taskOriginalHash = (Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash
    $taskLicenseHash = (Get-FileHash -LiteralPath $taskOldLicense -Algorithm SHA256).Hash
    $taskFixture = Start-Process -FilePath $taskApp -WindowStyle Hidden -PassThru
    # Prove that the fixture is actually running before asking Setup to update it.
    if ($taskFixture.WaitForExit(250)) { throw 'Lock fixture exited before the test.' }
    $taskForeign=Start-Process -FilePath $taskForeignApp -WindowStyle Hidden -PassThru
    if($taskForeign.WaitForExit(250)){throw 'Foreign same-name fixture exited prematurely.'}
    Invoke-TestProcess $taskInstaller @('/S', "/D=$taskInstallDir") 0
    if(-not $taskFixture.WaitForExit(3000)){throw 'Installer did not stop the nonresponsive target fixture.'}
    $taskFixture=$null
    if((Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $taskReleaseExe -Algorithm SHA256).Hash -or
        -not (Test-Path -LiteralPath $taskSentinel)){throw 'Automatic legacy replacement did not install the exact payload and preserve user files.'}
    $taskResults.Add(@{case='installer ends nonresponsive exact-path old process and upgrades';passed=$true})
    $taskForeign.Refresh();if($taskForeign.HasExited){throw 'Installer stopped a same-name process in a different directory.'}
    $taskResults.Add(@{case='same-name process outside target install directory is preserved';passed=$true})
    $taskOriginalHash=(Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash
    $taskLicenseHash=(Get-FileHash -LiteralPath $taskOldLicense -Algorithm SHA256).Hash

    # A read-only file should also fail clearly instead of being skipped.
    [IO.File]::SetAttributes($taskApp, [IO.FileAttributes]::ReadOnly)
    try { Invoke-TestProcess $taskInstaller @('/S', "/D=$taskInstallDir") 2 }
    finally { [IO.File]::SetAttributes($taskApp, [IO.FileAttributes]::Normal) }
    if ((Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash -ne $taskOriginalHash -or
        (Get-FileHash -LiteralPath $taskOldLicense -Algorithm SHA256).Hash -ne $taskLicenseHash -or
        -not (Test-Path -LiteralPath $taskRegistry)) { throw 'Permission failure changed the existing installation.' }
    $taskResults.Add(@{ case='read-only executable blocks install before writes'; passed=$true })

    foreach ($taskPhase in @('install after app exits', 'repeat upgrade')) {
        Invoke-TestProcess $taskInstaller @('/S', "/D=$taskInstallDir") 0
        if ((Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash -ne
            (Get-FileHash -LiteralPath $taskReleaseExe -Algorithm SHA256).Hash) { throw 'Installed executable differs from the release binary.' }
        $taskRegistration = Get-ItemProperty -LiteralPath (Join-Path $taskRegistry 'Install')
        if ($taskRegistration.InstallDir -ne $taskInstallDir) { throw 'Test registration points outside the test installation.' }
        if (-not (Test-Path -LiteralPath $taskSentinel)) { throw 'Upgrade removed the user sentinel.' }
        $taskResults.Add(@{ case=$taskPhase; passed=$true })
    }

    if ($SkipTrayLifecycle) {
        Write-Output 'Tray lifecycle checks skipped: this runner has no interactive desktop. Run without -SkipTrayLifecycle on an interactive Windows desktop.'
    } else {
        # A synthetic Debug tray app exercises the real exit handler without
        # opening the user's sessions or contending with the production singleton.
        Copy-Item -LiteralPath $taskLifecycleExe -Destination $taskApp -Force
        Start-IsolatedTray
        Invoke-TestProcess $taskReleaseExe @('--close-for-update',('"'+$taskApp+'"')) 0
        if(-not $taskFixture.WaitForExit(3000)){throw 'Graceful update signal did not close the hidden tray app.'}
        $taskFixture=$null
        $taskResults.Add(@{case='hidden tray app exits gracefully without termination fallback';passed=$true})
        Start-IsolatedTray
        Invoke-TestProcess $taskInstaller @('/S', "/D=$taskInstallDir") 0
        if(-not $taskFixture.WaitForExit(3000)){throw 'Installer did not automatically exit the hidden tray app.'}
        $taskFixture=$null
        if((Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $taskReleaseExe -Algorithm SHA256).Hash){throw 'Hidden tray upgrade did not replace the binary.'}
        $taskResults.Add(@{case='installer automatically exits hidden tray app before upgrade';passed=$true})
    }

    if ($PluginHostDirectory) {
        $taskManifest=Get-Content -LiteralPath (Join-Path $PluginHostDirectory 'runtime-manifest.json') -Encoding utf8 -Raw | ConvertFrom-Json
        foreach($taskFile in $taskManifest.files) {
            if((Get-FileHash -LiteralPath (Join-Path $taskInstallDir ('PluginHost\'+$taskFile.path)) -Algorithm SHA256).Hash -ne $taskFile.sha256){throw 'Installed host differs from the staged runtime.'}
        }
        $taskResults.Add(@{case='all installed host files match runtime manifest';passed=$true})
        $taskHost=Join-Path $taskInstallDir 'PluginHost\HomePanelPluginHost.exe'
        # Lock an installed DLL, which must stop upgrade before changing the main app.
        $taskLock=[IO.File]::Open((Join-Path $taskInstallDir 'PluginHost\hermes.dll'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        try {Invoke-TestProcess $taskInstaller @('/S', "/D=$taskInstallDir") 2}
        finally {$taskLock.Dispose()}
        if((Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $taskReleaseExe -Algorithm SHA256).Hash){throw 'Blocked host upgrade changed main executable.'}
        $taskResults.Add(@{case='locked host DLL blocks installation before any app write';passed=$true})
        $taskSavedPath=$env:PATH;$taskSavedRoaming=$env:APPDATA;$taskSavedLocal=$env:LOCALAPPDATA
        try {
            $env:PATH="$env:WINDIR\System32;$env:WINDIR"
            $env:APPDATA=Join-Path $taskRunRoot 'host-check-config';$env:LOCALAPPDATA=Join-Path $taskRunRoot 'host-check-local'
            Invoke-TestProcess $taskHost @('--self-test') 0
            $taskHostReport=Get-Content -LiteralPath (Join-Path $taskInstallDir 'PluginHost\host-self-test.json') -Encoding utf8 -Raw | ConvertFrom-Json
            if(-not $taskHostReport.passed){throw 'Installed host self-check failed.'}
            Copy-Item -LiteralPath (Join-Path $taskInstallDir 'PluginHost\host-self-test.json') -Destination (Join-Path $taskRunRoot 'installed-host-self-test.json')
            # This report is test-generated, not an installer payload; preserve unknown files on uninstall.
        } finally {$env:PATH=$taskSavedPath;$env:APPDATA=$taskSavedRoaming;$env:LOCALAPPDATA=$taskSavedLocal}
        $taskResults.Add(@{case='installed host starts with developer paths removed';passed=$true})
    }

    # Replace only our test-installed binary with the inert lock fixture.
    Copy-Item -LiteralPath $taskFixtureExe -Destination $taskApp -Force
    $taskFixture = Start-Process -FilePath $taskApp -WindowStyle Hidden -PassThru
    if ($taskFixture.WaitForExit(250)) { throw 'Uninstall lock fixture exited prematurely.' }
    $taskUninstaller = Join-Path $taskInstallDir 'Uninstall.exe'
    Invoke-TestProcess $taskUninstaller @('/S', "_?=$taskInstallDir") 2
    if (-not (Test-Path -LiteralPath $taskApp) -or -not (Test-Path -LiteralPath $taskOldLicense) -or
        -not (Test-Path -LiteralPath $taskRegistry)) { throw 'Blocked uninstall removed installed files or registration.' }
    $taskResults.Add(@{ case='running executable blocks uninstall before deletion'; passed=$true })
    Stop-TestFixture

    Invoke-TestProcess $taskUninstaller @('/S', "_?=$taskInstallDir") 0
    if ((Test-Path -LiteralPath $taskApp) -or (Test-Path -LiteralPath $taskRegistry) -or
        -not (Test-Path -LiteralPath $taskSentinel)) { throw 'Uninstall did not remove only the installed files and registration.' }
    $taskResults.Add(@{ case='uninstall succeeds after app exits and preserves unknown user file'; passed=$true })
    if($PluginHostDirectory -and (Test-Path -LiteralPath (Join-Path $taskInstallDir 'PluginHost/HomePanelPluginHost.exe'))){throw 'Uninstall left the host executable installed.'}
}
finally {
    Stop-TestFixture
    if($null -ne $taskForeign) {
        $taskForeign.Refresh()
        if(-not $taskForeign.HasExited) {
            if($taskForeign.Path -ne $taskForeignApp){throw 'Foreign fixture identity changed; will not stop it.'}
            Stop-Process -Id $taskForeign.Id
            $taskForeign.WaitForExit()
        }
    }
    $taskResults | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskRunRoot 'results.json') -Encoding utf8NoBOM
}
$taskResults | ConvertTo-Json -Depth 4
"Installer regression evidence: $taskRunRoot"
