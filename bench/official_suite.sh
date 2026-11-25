#!/usr/bin/env bash
# Reproducible public benchmark suite for Lancet across standardized open corpora:
#   1. Pinned Rust repository (`tokio-rs/tokio` tag `tokio-1.43.0`) — rg / fd / ast-grep vs lancet
#   2. Pinned TypeScript repository (`microsoft/TypeScript` shallow tag `v5.7.3`) — polyglot code & AST
#   3. Deterministic multi-format enterprise document suite (PDF, DOCX, XLSX, HWPX, ZIP) — rga vs lancet
#
# Uses `hyperfine` automatically if installed, and runs the full competitor matrix via Python 3 stdlib.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORP_DIR="$ROOT/bench/corpora"
IDX_DIR="$ROOT/bench/.cache/tokio-idx"

echo "[1/4] Building release binary (lancet)..."
cargo build --release --manifest-path "$ROOT/Cargo.toml"
LANCET="$ROOT/target/release/lancet"

mkdir -p "$CORP_DIR" "$ROOT/bench/.cache"

if [[ ! -d "$CORP_DIR/tokio" ]]; then
  echo "[2/4] Cloning pinned tokio-rs/tokio (tag: tokio-1.43.0)..."
  git clone --depth 1 --branch tokio-1.43.0 https://github.com/tokio-rs/tokio.git "$CORP_DIR/tokio"
else
  echo "[2/4] Found cached corpus: $CORP_DIR/tokio"
fi

echo "[3/4] Building Lancet trigram index on tokio-1.43.0..."
rm -rf "$IDX_DIR"
"$LANCET" index "$CORP_DIR/tokio" --index-dir "$IDX_DIR"

if command -v hyperfine >/dev/null 2>&1 && command -v rg >/dev/null 2>&1; then
  echo "[4/4] Running hyperfine head-to-head on tokio-1.43.0..."
  hyperfine --warmup 2 \
    "rg -i -l -F 'coop::poll_proceed' '$CORP_DIR/tokio'" \
    "'$LANCET' '\"coop::poll_proceed\" | count' '$CORP_DIR/tokio' --no-index" \
    "'$LANCET' '\"coop::poll_proceed\" | count' '$CORP_DIR/tokio' --index-dir '$IDX_DIR'" \
    "'$LANCET' '\"coop::poll_proceed\" | count' '$CORP_DIR/tokio' --trust-index --index-dir '$IDX_DIR'"
else
  echo "[4/4] Running built-in Python competitor benchmark suite..."
fi

python3 "$ROOT/bench/run_bench.py" --corpus "$CORP_DIR/tokio" --index-dir "$IDX_DIR"
python3 "$ROOT/bench/bench_competitors.py" --corpus "$CORP_DIR/tokio" --index-dir "$IDX_DIR"
