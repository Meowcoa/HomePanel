param([Parameter(Mandatory)][string]$OutputDirectory,[string]$ArchiveTool)
$ErrorActionPreference = 'Stop'
if(-not $ArchiveTool){$ArchiveTool=(Get-Command 7z.exe -ErrorAction Stop).Source}
$taskRoot=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $taskRoot){throw 'Use a new VC runtime preparation directory.'}
New-Item -ItemType Directory -Path $taskRoot | Out-Null
$taskArchive=Join-Path $taskRoot 'VC_redist.x64.exe'
Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/VC_redist.x64.exe' -OutFile $taskArchive
$taskSignature=Get-AuthenticodeSignature -LiteralPath $taskArchive
if($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch 'Microsoft Corporation'){throw 'VC archive publisher validation failed.'}
& node (Join-Path $PSScriptRoot 'extract-vc-cabinets.cjs') $taskArchive $taskRoot
if($LASTEXITCODE -ne 0){throw 'VC archive format validation failed.'}
& $ArchiveTool x (Join-Path $taskRoot 'container-0.cab') ('-o'+(Join-Path $taskRoot 'metadata')) -y -bso0
if($LASTEXITCODE -ne 0){throw 'VC metadata extraction failed.'}
& $ArchiveTool x (Join-Path $taskRoot 'container-1.cab') ('-o'+(Join-Path $taskRoot 'payloads')) -y -bso0
if($LASTEXITCODE -ne 0){throw 'VC payload extraction failed.'}
[xml]$taskXml=[IO.File]::ReadAllText((Join-Path $taskRoot 'metadata\0'))
$taskPayloads=@($taskXml.SelectNodes('//*[local-name()="Payload"]'))
$taskCab=@($taskPayloads | Where-Object {$_.FilePath -match 'vcRuntimeMinimum_amd64\\cab1\.cab$'})
$taskLicense=@($taskPayloads | Where-Object {$_.FilePath -eq 'license.rtf'})
if($taskCab.Count -ne 1 -or $taskLicense.Count -ne 1 -or $taskCab[0].SourcePath -notmatch '^a\d+$' -or $taskLicense[0].SourcePath -notmatch '^u\d+$'){throw 'VC archive manifest layout is unsupported.'}
& $ArchiveTool x (Join-Path $taskRoot ('payloads\'+$taskCab[0].SourcePath)) ('-o'+(Join-Path $taskRoot 'crt')) -y -bso0
if($LASTEXITCODE -ne 0){throw 'VC CRT extraction failed.'}
New-Item -ItemType Directory -Path (Join-Path $taskRoot 'redistributable-x64') | Out-Null
Get-ChildItem -LiteralPath (Join-Path $taskRoot 'crt') -File -Filter '*.dll_amd64' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $taskRoot ('redistributable-x64\'+($_.Name -replace '_amd64$','')))
}
Copy-Item -LiteralPath (Join-Path $taskRoot ('metadata\'+$taskLicense[0].SourcePath)) -Destination (Join-Path $taskRoot 'license.rtf')
[ordered]@{url='https://aka.ms/vs/17/release/VC_redist.x64.exe';sha256=(Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash;publisher=$taskSignature.SignerCertificate.Subject;installerExecuted=$false} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'provenance.json') -Encoding utf8NoBOM
