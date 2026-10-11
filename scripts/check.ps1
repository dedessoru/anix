param([string]$Toolchain = 'stable')
$ErrorActionPreference = 'Stop'
$anixRepoRoot = Split-Path -Parent $PSScriptRoot
$anixRustc = (& rustup which --toolchain $Toolchain rustc).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve the Rust toolchain.' }
$anixToolchainBin = Split-Path -Parent $anixRustc
$anixCargo = Join-Path $anixToolchainBin 'cargo.exe'
$anixPreviousRustc = $env:RUSTC
$anixPreviousRustdoc = $env:RUSTDOC
$anixPreviousPath = $env:Path
$anixPreviousToolchain = $env:RUSTUP_TOOLCHAIN
$anixPreviousLocation = Get-Location
try {
    Set-Location -LiteralPath $anixRepoRoot
    $env:RUSTC = $anixRustc
    $env:RUSTDOC = Join-Path $anixToolchainBin 'rustdoc.exe'
    $env:RUSTUP_TOOLCHAIN = $Toolchain
    $env:Path = "$anixToolchainBin;$anixPreviousPath"
    & $anixCargo fmt --all --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatter check failed.' }
    & $anixCargo check --workspace --target-dir target/verified
    if ($LASTEXITCODE -ne 0) { throw 'Compilation failed.' }
    & $anixCargo clippy --workspace --all-targets --target-dir target/verified -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    & $anixCargo test --workspace --target-dir target/verified
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
} finally {
    $env:RUSTC = $anixPreviousRustc
    $env:RUSTDOC = $anixPreviousRustdoc
    $env:RUSTUP_TOOLCHAIN = $anixPreviousToolchain
    $env:Path = $anixPreviousPath
    Set-Location -LiteralPath $anixPreviousLocation
}
