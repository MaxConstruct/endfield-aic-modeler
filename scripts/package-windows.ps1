# Builds a portable AIC Modeler .exe in target\dist\. The game data is built into the binary
# (crates\aic-editor\build.rs) and the C runtime is linked in, so the .exe runs on its own: no data
# folder, no Visual C++ redistributable, no console window.
#
#   pwsh scripts\package-windows.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$target = 'x86_64-pc-windows-msvc'
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value

# An explicit --target keeps the static C runtime off build scripts and proc macros.
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked -p aic-app-egui --target $target
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dist = 'target\dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$exe = Join-Path $dist "AIC-Modeler-$version-windows-x64.exe"
Copy-Item "target\$target\release\aic-app-egui.exe" $exe -Force
Write-Output $exe
