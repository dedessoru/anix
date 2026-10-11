param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
    [ValidateSet('test-lead','test-lead-4b')]
    [string]$ModelProfile = 'test-lead',
    [int]$Port = 0,
    [int]$ContextSize = 8192,
    [ValidateRange(0, 999)]
    [int]$GpuLayers = 0,
    [string]$RuntimePath = ''
)
$ErrorActionPreference = 'Stop'
$modelConfig = if ($ModelProfile -eq 'test-lead-4b') { @{Path='models/qwen3-4b/Qwen3-4B-Q4_K_M.gguf'; DefaultPort=8083} } else { @{Path='models/qwen3-8b/Qwen3-8B-Q4_K_M.gguf'; DefaultPort=8082} }
if ($Port -eq 0) { $Port = $modelConfig.DefaultPort }
$modelPath = Join-Path $RepositoryRoot $modelConfig.Path
$modelName = if ($ModelProfile -eq 'test-lead-4b') { 'local-test-lead-4b' } else { 'local-test-lead' }
$previousRustc = $env:RUSTC
try {
    $env:RUSTC = (& rustup which --toolchain stable rustc).Trim()
    & rustup run stable cargo run --manifest-path (Join-Path $RepositoryRoot 'Cargo.toml') --target-dir (Join-Path $RepositoryRoot 'target/verified') -p anix-cli -- --workspace $RepositoryRoot model bootstrap --profile $ModelProfile
    if ($LASTEXITCODE -ne 0) { throw 'Test Lead model bootstrap failed' }
} finally { $env:RUSTC = $previousRustc }
$runtime = if ($RuntimePath) { $RuntimePath } else { Join-Path $RepositoryRoot '.runtime/llama-b11307/llama-server.exe' }
if (-not (Test-Path -LiteralPath $runtime)) {
    & (Join-Path $PSScriptRoot 'bootstrap-runtime.ps1') -RepositoryRoot $RepositoryRoot | Out-Null
}
if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
    throw "Port $Port is already in use; refusing to attach to an unverified server"
}
$arguments = @('--model', $modelPath, '--host', '127.0.0.1', '--port', "$Port", '--ctx-size', "$ContextSize", '--parallel', '1', '--n-gpu-layers', "$GpuLayers", '--jinja', '--no-webui', '--alias', $modelName)
$server = Start-Process -FilePath $runtime -ArgumentList $arguments -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $RepositoryRoot '.runtime/test-lead.stdout.log') -RedirectStandardError (Join-Path $RepositoryRoot '.runtime/test-lead.stderr.log')
try {
    $endpoint = "http://127.0.0.1:$Port"
    $ready = $false
    for ($i = 0; $i -lt 900; $i++) {
        if ($server.HasExited) { throw "llama-server exited with code $($server.ExitCode)" }
        try {
            $health = Invoke-RestMethod -Uri "$endpoint/health" -TimeoutSec 2
            if ($health.status -eq 'ok') { $ready = $true; break }
        } catch { Start-Sleep -Seconds 2 }
    }
    if (-not $ready) { throw 'Test Lead server did not become ready within 30 minutes' }
    $payload = @{ model=$modelName; messages=@(@{role='user';content='Reply with the JSON object {"ready":true} and no other text.'}); temperature=0; max_tokens=64; response_format=@{type='json_object'}; chat_template_kwargs=@{enable_thinking=$false}; stream=$false } | ConvertTo-Json -Depth 8
    $completion = Invoke-RestMethod -Method Post -Uri "$endpoint/v1/chat/completions" -ContentType 'application/json' -Body $payload -TimeoutSec 300
    if (-not $completion.choices[0].message.content) { throw 'Test Lead completion smoke returned no content' }
    Set-Content -LiteralPath (Join-Path $RepositoryRoot '.runtime/test-lead.pid') -Value $server.Id -Encoding ascii
    Write-Output "Test Lead ready: $endpoint/v1 (pid $($server.Id))"
    Write-Output "Smoke completion: $($completion.choices[0].message.content)"
} catch {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    throw
}
