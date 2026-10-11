#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${RN_TEST_LEAD_MODEL_PROFILE:-test-lead}"
if [[ "$profile" == "test-lead-4b" ]]; then
  default_port=8083
  model="$root/models/qwen3-4b/Qwen3-4B-Q4_K_M.gguf"
  model_name=local-test-lead-4b
else
  profile=test-lead
  default_port=8082
  model="$root/models/qwen3-8b/Qwen3-8B-Q4_K_M.gguf"
  model_name=local-test-lead
fi
port="${RN_TEST_LEAD_PORT:-$default_port}"
context_size="${RN_TEST_LEAD_CONTEXT:-8192}"
cargo run --manifest-path "$root/Cargo.toml" -p anix-cli -- --workspace "$root" model bootstrap --profile "$profile"
server="${RN_LLAMA_SERVER:-$(command -v llama-server || true)}"
if [[ -z "$server" ]]; then server="$root/.runtime/llama-b11307/llama-server"; fi
if [[ ! -x "$server" ]]; then
  echo "llama-server executable missing; install a CPU llama.cpp server at $server or set RN_LLAMA_SERVER" >&2
  exit 1
fi
if (command -v ss >/dev/null && ss -ltn "sport = :$port" | grep -q LISTEN); then
  echo "Port $port is already in use; refusing to attach to an unverified server" >&2
  exit 1
fi
mkdir -p "$root/.runtime"
"$server" --model "$model" --host 127.0.0.1 --port "$port" --ctx-size "$context_size" --parallel 1 --n-gpu-layers 0 --jinja --no-webui --alias "$model_name" >"$root/.runtime/test-lead.log" 2>&1 &
pid=$!
echo "$pid" > "$root/.runtime/test-lead.pid"
cleanup() { kill "$pid" 2>/dev/null || true; }
trap cleanup EXIT
endpoint="http://127.0.0.1:$port"
ready=false
for _ in $(seq 1 900); do
  if ! kill -0 "$pid" 2>/dev/null; then echo "llama-server exited; see .runtime/test-lead.log" >&2; exit 1; fi
  if curl -fsS "$endpoint/health" | grep -q '"status"[[:space:]]*:[[:space:]]*"ok"'; then ready=true; break; fi
  sleep 2
done
if [[ "$ready" != true ]]; then echo "Test Lead server did not become ready within 30 minutes" >&2; exit 1; fi
curl -fsS "$endpoint/v1/chat/completions" -H 'Content-Type: application/json' -d "{\"model\":\"$model_name\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply only with {\\\"ready\\\":true}.\"}],\"temperature\":0,\"max_tokens\":64,\"response_format\":{\"type\":\"json_object\"},\"chat_template_kwargs\":{\"enable_thinking\":false},\"stream\":false}" > "$root/.runtime/test-lead-smoke.json"
grep -q '"content"' "$root/.runtime/test-lead-smoke.json"
echo "Test Lead ready: $endpoint/v1 (pid $pid)"
cat "$root/.runtime/test-lead-smoke.json"
trap - EXIT
