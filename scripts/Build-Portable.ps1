param([string]$WorkspaceRoot)

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $WorkspaceRoot) { $WorkspaceRoot = Join-Path $projectRoot '..\..' }
$workspace = [IO.Path]::GetFullPath($WorkspaceRoot)
$manifest = Join-Path $projectRoot 'src-tauri\Cargo.toml'
$metadata = cargo metadata --locked --no-deps --format-version 1 --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve Cargo version.' }
$version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'homepanel').version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid application version.' }
$target = Join-Path $workspace 'work\cargo-portable'
cargo build --locked --release --manifest-path $manifest --target x86_64-pc-windows-msvc --target-dir $target --jobs 2 --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'
if ($LASTEXITCODE -ne 0) { throw 'Standalone build failed.' }
$built = Join-Path $target 'x86_64-pc-windows-msvc\release\homepanel.exe'
$dumpbin = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
if (-not $dumpbin) { throw 'dumpbin.exe is required to verify standalone dependencies. Use an MSVC developer shell.' }
$imports = & $dumpbin.Source /dependents $built
if ($LASTEXITCODE -ne 0) { throw 'Unable to inspect executable dependencies.' }
if (($imports -join "`n") -match '(?i)VCRUNTIME\d*|MSVCP\d*|ucrtbase\.dll|api-ms-win-crt-') {
    throw 'A dynamic Visual C++ runtime dependency remains; standalone validation failed.'
}
$out = Join-Path $workspace "work\HomePanel-release-$version"
New-Item -ItemType Directory -Path $out -Force | Out-Null
$exe = Join-Path $out 'HomePanel.exe'
Copy-Item -LiteralPath $built -Destination $exe
$imports | Set-Content -LiteralPath (Join-Path $out 'PE-dependencies.txt') -Encoding utf8NoBOM
Get-Item -LiteralPath $exe | Select-Object FullName,Length
