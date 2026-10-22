param([string]$MSBuildPath, [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if (-not $MSBuildPath) {
    $taskCommand=Get-Command MSBuild.exe -ErrorAction SilentlyContinue
    if (-not $taskCommand) {throw 'Run from the VS 2022 C++ Developer PowerShell, or pass -MSBuildPath.'}
    $MSBuildPath=$taskCommand.Source
}
if (-not $OutputDirectory) {throw 'Pass an output directory outside the source tree.'}
$taskOutput=[IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\') + '\'
$taskSource=[IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\') + '\'
if ($taskOutput.StartsWith($taskSource,[StringComparison]::OrdinalIgnoreCase)) {throw 'Build output must be outside this source tree.'}
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskPatchBackup=Join-Path $taskOutput ('svg-fabric-source-backup-'+[guid]::NewGuid().ToString('N'))
$taskPatched=$false
Push-Location -LiteralPath $PSScriptRoot
try {
    & node '.\prepareSvgFabric.cjs' apply $PSScriptRoot $taskPatchBackup
    if ($LASTEXITCODE -ne 0) {throw 'SVG Fabric build preparation failed; no unknown sources are overwritten.'}
    $taskPatched=$true
    & node '.\node_modules\@react-native-community\cli\build\bin.js' autolink-windows
    if ($LASTEXITCODE -ne 0) {throw 'Windows autolinking failed'}
    & $MSBuildPath '.\windows\HomePanelPluginHost.native.slnf' /restore /p:Configuration=Release /p:Platform=x64 /p:UseFabric=true /p:RnwNewArch=true "/p:OutDir=$taskOutput" "/p:IntermediateDirectoryRoot=$($taskOutput)intermediate\" "/p:BundleOutputPath=$($taskOutput)Bundle" /p:BundleContent= /m:2 /nr:false /nologo
    if ($LASTEXITCODE -ne 0) {throw "Native host build failed: $LASTEXITCODE"}
} finally {
    try {
        if ($taskPatched) {
            & node '.\prepareSvgFabric.cjs' restore $taskPatchBackup
            if ($LASTEXITCODE -ne 0) {throw "SVG source restoration needs attention; originals remain at $taskPatchBackup"}
        }
    } finally {Pop-Location}
}
