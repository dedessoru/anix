#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo run --manifest-path "$root/Cargo.toml" -p anix-cli -- --workspace "$root" model bootstrap
