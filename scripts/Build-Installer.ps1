param(
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.4.11',
    [switch]$TestMode,
    [string]$ReleaseExe,
    [string]$PluginHostDirectory,
    [string]$WorkspaceRoot
)

$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $workspaceRoot = [System.IO.Path]::GetFullPath((Join-Path $projectRoot '..\..'))
} else {
    $workspaceRoot = [System.IO.Path]::GetFullPath($WorkspaceRoot)
}
$releaseExe = if ($ReleaseExe) { [IO.Path]::GetFullPath($ReleaseExe) } else { Join-Path $workspaceRoot 'work\cargo-target\release\homepanel.exe' }
$outputDirectory = Join-Path $workspaceRoot "work\HomePanel-release-$Version"
$testStartMenu = Join-Path $workspaceRoot 'work\HomePanel-installer-test-support\StartMenu'
$scriptPath = Join-Path $projectRoot 'packaging\HomePanel.nsi'

if (-not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) {
    throw "Release executable not found: $releaseExe"
}
if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
    throw "NSIS script not found: $scriptPath"
}

$nsis = Get-Command 'makensis.exe' -ErrorAction SilentlyContinue
$nsisRoot = $env:NSISDIR
if (-not $nsisRoot -and $nsis) {
    $nsisRoot = Split-Path -Parent $nsis.Source
}
if (-not $nsisRoot) {
    throw 'NSIS 3.13 was not found. Set NSISDIR or add makensis.exe to PATH.'
}
$makensisPath = Join-Path $nsisRoot 'makensis.exe'
if (-not (Test-Path -LiteralPath $makensisPath -PathType Leaf)) {
    throw "makensis.exe not found under NSISDIR: $nsisRoot"
}
$env:NSISDIR = $nsisRoot
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null

$versionParts = $Version.Split('.')
$numericVersion = "$($versionParts[0]).$($versionParts[1]).$($versionParts[2]).0"
$nsisArguments = @(
    '/V2',
    '/WX',
    '/INPUTCHARSET', 'UTF8',
    "/DVERSION=$Version",
    "/DVERSION_NUM=$numericVersion",
    "/DOUTPUT_DIR=$outputDirectory",
    "/DRELEASE_EXE=$releaseExe",
    "/DSOURCE_ROOT=$projectRoot"
)
if ($PluginHostDirectory) {
    $taskInclude=Join-Path $outputDirectory ('HostFiles'+$(if($TestMode){'-test'}else{''})+'.nsh')
    & (Join-Path $PSScriptRoot 'New-HostInstallerInclude.ps1') -PluginHostDirectory $PluginHostDirectory -OutputPath $taskInclude
    $nsisArguments += "/DHOST_INCLUDE=$taskInclude"
}
if ($TestMode) {
    $nsisArguments += '/DTEST_MODE'
    $nsisArguments += "/DTEST_START_MENU=$testStartMenu"
}
$nsisArguments += $scriptPath
& $makensisPath @nsisArguments
if ($LASTEXITCODE -ne 0) {
    throw "makensis failed with exit code $LASTEXITCODE"
}

$testSuffix = if ($TestMode) { '-test' } else { '' }
$installer = Join-Path $outputDirectory "HomePanel-Setup-$Version$testSuffix.exe"
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
    throw "NSIS finished without creating the installer: $installer"
}
Get-Item -LiteralPath $installer | Select-Object FullName, Length, LastWriteTime
