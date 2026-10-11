param([string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference = 'Stop'
$runtimeRoot = Join-Path $RepositoryRoot '.runtime'
$runtimeDestination = Join-Path $runtimeRoot 'llama-b11307'
$runtimeExecutable = Join-Path $runtimeDestination 'llama-server.exe'
if (Test-Path -LiteralPath $runtimeExecutable) { Write-Output $runtimeExecutable; exit 0 }
New-Item -ItemType Directory -Force -Path $runtimeRoot | Out-Null
$runtimeArchive = Join-Path $runtimeRoot 'llama-b11307.zip'
Invoke-WebRequest -UseBasicParsing 'https://github.com/ggml-org/llama.cpp/releases/download/b11307/llama-b11307-bin-win-cpu-x64.zip' -OutFile $runtimeArchive
$runtimeChecksum = (Get-FileHash -LiteralPath $runtimeArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($runtimeChecksum -ne '558e7563b9816ab7e0ce3cbd91e8f76507a64dc96abc8871989fb67c5f525d30') { throw 'llama.cpp archive checksum mismatch' }
Expand-Archive -LiteralPath $runtimeArchive -DestinationPath $runtimeDestination -Force
if (-not (Test-Path -LiteralPath $runtimeExecutable)) { throw 'llama-server.exe missing in verified archive' }
Write-Output $runtimeExecutable
