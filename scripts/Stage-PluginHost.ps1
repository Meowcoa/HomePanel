param(
    [Parameter(Mandatory)][string]$NativeOutput,
    [Parameter(Mandatory)][string]$VcRuntimeDirectory,
    [Parameter(Mandatory)][string]$VcLicense,
    [Parameter(Mandatory)][string]$NuGetPackages,
    [Parameter(Mandatory)][string]$HostNodeModules,
    [Parameter(Mandatory)][string]$BundleSourceMap,
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference = 'Stop'
$taskSource=[IO.Path]::GetFullPath($NativeOutput)
$taskDestination=[IO.Path]::GetFullPath($Destination)
if(Test-Path -LiteralPath $taskDestination){throw 'Use a new staging directory; existing contents are preserved.'}
foreach($taskRequired in @('HomePanelPluginHost.exe','Microsoft.ReactNative.dll','hermes.dll','Bundle\index.windows.bundle')) {
    if(-not(Test-Path -LiteralPath (Join-Path $taskSource $taskRequired) -PathType Leaf)){throw "Missing host output: $taskRequired"}
}
New-Item -ItemType Directory -Path $taskDestination | Out-Null
Get-ChildItem -LiteralPath $taskSource -Recurse -File | ForEach-Object {
    $taskRelative=[IO.Path]::GetRelativePath($taskSource,$_.FullName)
    $taskParts=$taskRelative.Split([IO.Path]::DirectorySeparatorChar)
    $taskAllowed=($taskParts.Count -eq 1 -and ($_.Extension -in @('.dll','.pri','.winmd','.png') -or $_.Name -in @('HomePanelPluginHost.exe','RestartAgent.exe'))) -or
        ($taskParts.Count -eq 2 -and $taskParts[0] -match '^[a-z]{2,3}-(?:[A-Z]{2}|[A-Za-z]+)(?:-[A-Za-z]+)?$' -and $_.Extension -in @('.dll','.pri')) -or
        $taskRelative -eq 'Bundle\index.windows.bundle'
    if($taskAllowed) {
        $taskTarget=Join-Path $taskDestination $taskRelative
        New-Item -ItemType Directory -Path (Split-Path -Parent $taskTarget) -Force | Out-Null
        Copy-Item -LiteralPath $_.FullName -Destination $taskTarget
    }
}
$taskLicenseDirectory=Join-Path $taskDestination 'LICENSES'
New-Item -ItemType Directory -Path $taskLicenseDirectory | Out-Null
foreach($taskName in @('msvcp140','msvcp140_1','msvcp140_2','msvcp140_atomic_wait','msvcp140_codecvt_ids','vcruntime140','vcruntime140_1','vcruntime140_threads','concrt140')) {
    $taskFile=Join-Path $VcRuntimeDirectory ($taskName+'.dll')
    $taskSignature=Get-AuthenticodeSignature -LiteralPath $taskFile
    if($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch 'Microsoft Corporation'){throw "Unverified redistributable: $taskName"}
    Copy-Item -LiteralPath $taskFile -Destination $taskDestination
}
Copy-Item -LiteralPath $VcLicense -Destination (Join-Path $taskLicenseDirectory 'Microsoft-VC-runtime.rtf')
$taskHostSource=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\experiments\rnw-host'))
Copy-Item -LiteralPath (Join-Path $taskHostSource 'LICENSE-RNW.txt') -Destination $taskLicenseDirectory
Copy-Item -LiteralPath (Join-Path $taskHostSource 'LICENSE-SVG.txt') -Destination $taskLicenseDirectory
foreach($taskPackage in @('microsoft.windowsappsdk','microsoft.javascript.hermes','microsoft.web.webview2')) {
    $taskPackageRoot=Join-Path $NuGetPackages $taskPackage
    $taskVersion=@{'microsoft.windowsappsdk'='1.7.250401001';'microsoft.javascript.hermes'='0.0.0-2505.2001-0e4bc3b9';'microsoft.web.webview2'='1.0.2903.40'}[$taskPackage]
    $taskVersionRoot=Get-Item -LiteralPath (Join-Path $taskPackageRoot $taskVersion)
    Get-ChildItem -LiteralPath $taskVersionRoot.FullName -File | Where-Object {$_.Name -match '^(LICENSE|NOTICE)(\.txt)?$'} | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $taskLicenseDirectory ($taskPackage+'-'+$_.Name))
    }
}
& node (Join-Path $PSScriptRoot 'generate-host-licenses.cjs') $HostNodeModules (Join-Path $taskLicenseDirectory 'JavaScript-dependencies.txt') $BundleSourceMap
if($LASTEXITCODE -ne 0){throw 'Host license inventory failed'}
$taskFiles=Get-ChildItem -LiteralPath $taskDestination -Recurse -File | Sort-Object FullName | ForEach-Object {
    [ordered]@{path=[IO.Path]::GetRelativePath($taskDestination,$_.FullName);bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
}
[ordered]@{schema=1;rnWindows='0.79.5';windowsAppSdk='1.7.250401001';containsXiaomiCode=$false;files=@($taskFiles)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskDestination 'runtime-manifest.json') -Encoding utf8NoBOM
Write-Output "Staged $(@($taskFiles).Count) runtime files at $taskDestination"
