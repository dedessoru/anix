param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$Profile = 'micro'
)
$ErrorActionPreference = 'Stop'
$previousRustc = $env:RUSTC
try {
    $env:RUSTC = (& rustup which --toolchain stable rustc).Trim()
    & rustup run stable cargo run --manifest-path (Join-Path $RepositoryRoot 'Cargo.toml') --target-dir (Join-Path $RepositoryRoot 'target/verified') -p anix-cli -- --workspace $RepositoryRoot model bootstrap --profile $Profile
    if ($LASTEXITCODE -ne 0) { throw 'Model bootstrap failed' }
} finally { $env:RUSTC = $previousRustc }
