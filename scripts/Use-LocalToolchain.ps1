$ErrorActionPreference = 'Stop'
# Optional workspace-only bootstrap. Normal development uses rustup + MSVC
# installed in the developer's environment and does not require this script.
$taskToolRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..\work\toolchains'))
$taskCargoBin = Join-Path $taskToolRoot 'native\bin'
if (-not (Test-Path -LiteralPath (Join-Path $taskCargoBin 'cargo.exe'))) { throw 'Workspace Rust toolchain not present. Install Rust/MSVC normally instead.' }
$env:RUSTUP_HOME = Join-Path $taskToolRoot 'rustup'
$env:CARGO_HOME = Join-Path $taskToolRoot 'cargo'
$env:RUSTC = Join-Path $taskCargoBin 'rustc.exe'
$env:RUSTDOC = Join-Path $taskCargoBin 'rustdoc.exe'
$env:CARGO_TARGET_DIR = Join-Path $taskToolRoot '..\cargo-target'
$taskMsvcBase = Join-Path $taskToolRoot 'msvc\VC\Tools\MSVC'
$taskMsvc = (Get-ChildItem -LiteralPath $taskMsvcBase -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName
$taskSdkRoot = Join-Path $taskToolRoot 'msvc\Windows Kits\10'
$taskSdkVersion = (Get-ChildItem -LiteralPath (Join-Path $taskSdkRoot 'Lib') -Directory | Sort-Object Name -Descending | Select-Object -First 1).Name
$taskMsvcBin = Join-Path $taskMsvc 'bin\Hostx64\x64'
$taskSdkBin = Join-Path $taskSdkRoot "bin\$taskSdkVersion\x64"
$env:PATH = "$taskCargoBin;$taskMsvcBin;$taskSdkBin;$env:PATH"
$env:INCLUDE = "$taskMsvc\include;$taskSdkRoot\Include\$taskSdkVersion\ucrt;$taskSdkRoot\Include\$taskSdkVersion\shared;$taskSdkRoot\Include\$taskSdkVersion\um;$taskSdkRoot\Include\$taskSdkVersion\winrt"
$env:LIB = "$taskMsvc\lib\x64;$taskSdkRoot\Lib\$taskSdkVersion\ucrt\x64;$taskSdkRoot\Lib\$taskSdkVersion\um\x64"
$env:VCToolsInstallDir = "$taskMsvc\"
$env:WindowsSdkDir = "$taskSdkRoot\"
$env:WindowsSDKVersion = "$taskSdkVersion\"
$env:VSCMD_ARG_HOST_ARCH = 'x64'
$env:VSCMD_ARG_TGT_ARCH = 'x64'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $taskMsvcBin 'link.exe'
$env:RC = Join-Path $taskSdkBin 'rc.exe'
$env:CC = Join-Path $taskMsvcBin 'cl.exe'
$env:CXX = $env:CC
