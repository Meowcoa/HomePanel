param([Parameter(Mandatory)][string]$PluginHostDirectory,[Parameter(Mandatory)][string]$OutputPath)
$ErrorActionPreference = 'Stop'
$taskRoot=[IO.Path]::GetFullPath($PluginHostDirectory)
$taskManifest=Get-Content -LiteralPath (Join-Path $taskRoot 'runtime-manifest.json') -Encoding utf8 -Raw | ConvertFrom-Json
if($taskManifest.schema -ne 1 -or $taskManifest.containsXiaomiCode -ne $false){throw 'Invalid host runtime manifest.'}
$taskPaths=@($taskManifest.files | ForEach-Object {
    $taskRelative=$_.path
    if($taskRelative -notmatch '^[A-Za-z0-9_. -]+(\\[A-Za-z0-9_. -]+)*$' -or $taskRelative -match '(^|\\)\.\.(\\|$)' -or $taskRelative -match '(?i)LocalPlugin|sdk\.bundle|switch\.bundle'){throw 'Unsafe runtime payload path.'}
    $taskFile=Join-Path $taskRoot $taskRelative
    if((Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash -ne $_.sha256){throw "Runtime payload changed: $taskRelative"}
    $taskRelative
}) + 'runtime-manifest.json'
$taskLines=[Collections.Generic.List[string]]::new()
$taskLines.Add('!macro HomePanelHostInstall')
foreach($taskRelative in $taskPaths) {
    $taskParent=Split-Path -Parent $taskRelative
    $taskName=Split-Path -Leaf $taskRelative
    $taskLines.Add('SetOutPath "$INSTDIR\PluginHost'+$(if($taskParent){'\'+$taskParent}else{''})+'"')
    $taskLines.Add('File "/oname='+$taskName+'" "'+(Join-Path $taskRoot $taskRelative)+'"')
}
$taskLines.Add('!macroend')
$taskLines.Add('!macro HomePanelHostDelete')
foreach($taskRelative in $taskPaths){$taskLines.Add('Delete "$INSTDIR\PluginHost\'+$taskRelative+'"')}
$taskDirectories=@($taskPaths | ForEach-Object {Split-Path -Parent $_} | Where-Object {$_} | Sort-Object -Unique -Descending)
foreach($taskRelative in $taskDirectories){$taskLines.Add('RMDir "$INSTDIR\PluginHost\'+$taskRelative+'"')}
$taskLines.Add('RMDir "$INSTDIR\PluginHost"')
$taskLines.Add('!macroend')
$taskLines.Add('!macro HomePanelHostGuard')
$taskIndex=0
foreach($taskRelative in $taskPaths) {
    $taskIndex++
    $taskFile='$INSTDIR\PluginHost\'+$taskRelative
    $taskLines.Add('IfFileExists "'+$taskFile+'" 0 host_ready_'+$taskIndex)
    $taskLines.Add("System::Call 'kernel32::CreateFileW(w `"$taskFile`", i 0x40000000, i 7, p 0, i 3, i 0x80, p 0) p .r0 ?e'")
    $taskLines.Add('Pop $1')
    $taskLines.Add('IntPtrCmp $0 -1 blocked')
    $taskLines.Add("System::Call 'kernel32::CloseHandle(p r0)'")
    $taskLines.Add('host_ready_'+$taskIndex+':')
}
$taskLines.Add('!macroend')
$taskLines | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
