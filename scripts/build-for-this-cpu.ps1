# Builds AIC Modeler for this PC's own CPU: the `packaged` profile (fat LTO, one codegen unit) with
# `-C target-cpu=native`, so the compiler may use every instruction this CPU has. The .exe may not
# run on another PC: it is for your own use, never for sharing. `cargo build --release`, the
# packaging scripts and the web build don't use it (PLAN.md, M13 step 6).
#
#   pwsh scripts\build-for-this-cpu.ps1
#   powershell -File scripts\build-for-this-cpu.ps1   # Windows PowerShell 5.1
#
# It builds into target\native\ so the usual builds keep their caches (a different RUSTFLAGS
# rebuilds everything). About 6 minutes. Cargo's exit code is checked rather than setting
# $ErrorActionPreference to Stop: Windows PowerShell 5.1 would stop on cargo's progress, which it
# prints on stderr.
Set-Location (Join-Path $PSScriptRoot '..')

$env:RUSTFLAGS = '-C target-cpu=native'
cargo build --profile packaged --locked -p aic-app --target-dir target\native
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output (Resolve-Path 'target\native\packaged\aic-app.exe')
