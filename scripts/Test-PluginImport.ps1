param([Parameter(Mandatory)][string]$ReleaseExe,[Parameter(Mandatory)][string]$ApkPath,[Parameter(Mandatory)][string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
$taskRoot=[IO.Path]::GetFullPath($EvidenceDirectory)
if(Test-Path -LiteralPath $taskRoot){throw 'Use a new import test directory.'}
New-Item -ItemType Directory -Path $taskRoot | Out-Null
function Invoke-Import([string]$Source,[int]$Expected) {
    $taskProcess=Start-Process -FilePath $ReleaseExe -ArgumentList @('--import-plugin-apk',('"'+$Source+'"'),'--plugin-data-dir',('"'+$taskRoot+'"')) -WindowStyle Hidden -PassThru
    if(-not $taskProcess.WaitForExit(20000) -or $taskProcess.ExitCode -ne $Expected){throw 'Unexpected offline importer result.'}
}
Invoke-Import ([IO.Path]::GetFullPath($ApkPath)) 0
$taskLocal=Join-Path $taskRoot 'local-plugin-v1'
$taskHashes=@{}
Get-ChildItem -LiteralPath $taskLocal -File -Recurse | ForEach-Object {$taskHashes[[IO.Path]::GetRelativePath($taskLocal,$_.FullName)]=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
if($taskHashes['sdk.bundle'] -ne 'aefa10092eb349175a21ea4025f2f8352a4c4461ed05324b4cfb1a2b2b598c01' -or $taskHashes['switch.bundle'] -ne '38f58139ff556c9f0e576962a7e51f885cf508da626e939d9516344d014aa11f'){throw 'Imported source differs from the pinned sample.'}
$taskAssets=Get-Content -LiteralPath (Join-Path $taskLocal 'local-assets.json') -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable
if($taskAssets.Count -lt 1){throw 'No local images were imported.'}
foreach($taskAsset in $taskAssets.Values){if(-not(Test-Path -LiteralPath (Join-Path $taskLocal ('images\'+$taskAsset.name)))){throw 'Resource map references a missing image.'}}
$taskInvalid=Join-Path $taskRoot 'unsupported.apk'
[IO.File]::WriteAllText($taskInvalid,'not an APK',[Text.UTF8Encoding]::new($false))
Invoke-Import $taskInvalid 2
foreach($taskRelative in $taskHashes.Keys){if((Get-FileHash -LiteralPath (Join-Path $taskLocal $taskRelative) -Algorithm SHA256).Hash -ne $taskHashes[$taskRelative]){throw 'Rejected import changed the working local candidate.'}}
[ordered]@{passed=$true;pinnedSdkAndPlugin=$true;localRasterImages=$taskAssets.Count;rejectedImportPreservedWorkingFiles=$true;realAccountAccessed=$false;publisherSignatureVerified=$false} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'results.json') -Encoding utf8NoBOM
Get-Content -LiteralPath (Join-Path $taskRoot 'results.json') -Encoding utf8
