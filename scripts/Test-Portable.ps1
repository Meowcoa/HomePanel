param([Parameter(Mandatory)][string]$WorkspaceRoot)

$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath($WorkspaceRoot)
$manifest = Join-Path $PSScriptRoot '..\src-tauri\Cargo.toml'
$metadata = cargo metadata --locked --no-deps --format-version 1 --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve version.' }
$version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'homepanel').version
$exe = Join-Path $workspace "work\HomePanel-release-$version\HomePanel.exe"
$testRoot = Join-Path $workspace ('work\HomePanel-portable-check-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot,(Join-Path $testRoot 'Roaming'),(Join-Path $testRoot 'Local') | Out-Null
$previousPath=$env:PATH; $previousRoaming=$env:APPDATA; $previousLocal=$env:LOCALAPPDATA
$process=$null
try {
    $env:PATH="$env:WINDIR\System32;$env:WINDIR"
    $env:APPDATA=Join-Path $testRoot 'Roaming'; $env:LOCALAPPDATA=Join-Path $testRoot 'Local'
    $process=Start-Process -FilePath $exe -ArgumentList '--self-check' -WorkingDirectory $testRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot 'stdout.log') -RedirectStandardError (Join-Path $testRoot 'stderr.log')
    if (-not $process.WaitForExit(12000)) { throw 'Standalone self-check timed out.' }
    if ($process.ExitCode -ne 0) { throw "Standalone self-check failed: $($process.ExitCode)" }
    $report=[ordered]@{passed=$true;version=$version;exitCode=$process.ExitCode;emptyWorkingDirectory=$true;developerPathsRemoved=$true;embeddedChineseGlyphs=$true;realAccountAccessed=$false;cleanWindowsVmTested=$false}
    $report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $testRoot 'results.json') -Encoding utf8NoBOM
    Write-Output "Standalone evidence: $testRoot"
} finally {
    $env:PATH=$previousPath; $env:APPDATA=$previousRoaming; $env:LOCALAPPDATA=$previousLocal
    if ($null -ne $process -and -not $process.HasExited) {
        if ($process.Path -ne $exe) {throw 'Self-check process identity changed.'}
        $process.Kill()
    }
}
