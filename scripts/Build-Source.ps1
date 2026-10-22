param([Parameter(Mandatory)][string]$OutputPath)
$ErrorActionPreference = 'Stop'
$taskRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskOutput=[IO.Path]::GetFullPath($OutputPath)
if(Test-Path -LiteralPath $taskOutput){throw 'Use a new source archive path.'}
$taskFiles=[Collections.Generic.List[IO.FileInfo]]::new()
foreach($taskDirectory in @('.github','adapters','LICENSES','scripts','packaging','src-tauri\src','src-tauri\assets','src-tauri\icons','experiments\rnw-host')) {
    Get-ChildItem -LiteralPath (Join-Path $taskRoot $taskDirectory) -Recurse -File | Where-Object {$_.FullName -notmatch '[\\/](node_modules|target|obj|bin|LocalPlugin|local-plugin-v\d+)[\\/]' -and $_.Extension -notin @('.log','.apk','.dpapi','.zip','.mpkg','.mpk','.hbc')} | ForEach-Object {$taskFiles.Add($_)}
}
foreach($taskName in @('LICENSE','AGENTS.md','README.md','RELEASE_NOTES_0.4.11.md','COMPATIBILITY.md','PLUGIN_COMPATIBILITY.md','PLUGIN_DOWNLOAD.md','MIJIA_API_COVERAGE.md','SPEAKER_SERVICE.md','APK_ANALYSIS.md','THIRD_PARTY_NOTICES.md','references.json','.gitignore','src-tauri\Cargo.toml','src-tauri\Cargo.lock','src-tauri\build.rs')) {$taskFiles.Add((Get-Item -LiteralPath (Join-Path $taskRoot $taskName)))}
$taskZip=[IO.Compression.ZipFile]::Open($taskOutput,[IO.Compression.ZipArchiveMode]::Create)
try {
    foreach($taskFile in $taskFiles | Sort-Object FullName -Unique) {
        $taskRelative=[IO.Path]::GetRelativePath($taskRoot,$taskFile.FullName).Replace('\','/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($taskZip,$taskFile.FullName,$taskRelative,[IO.Compression.CompressionLevel]::Optimal)|Out-Null
    }
} finally {$taskZip.Dispose()}
Get-Item -LiteralPath $taskOutput | Select-Object FullName,Length
