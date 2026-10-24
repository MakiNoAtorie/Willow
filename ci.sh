#!/bin/sh
# Runs the CI checks locally (macOS/Linux twin of ci.bat). Pass the cargo job count (default 4; use 2 when another build runs).
set -e
cd "$(dirname "$0")"
J=${1:-4}
if [ -f Cargo.toml ]; then
  cargo fmt --all --check
  cargo build -j "$J" --workspace --all-targets
  cargo test -j "$J" --workspace -- --test-threads=2
  cargo clippy -j "$J" --workspace --all-targets -- -D warnings
  # Windows is verified by hand later; keep its code compiling meanwhile.
  if rustup target list --installed 2>/dev/null | grep -q '^x86_64-pc-windows-gnu$'; then
    cargo clippy -j "$J" -p willow-app --target x86_64-pc-windows-gnu -- -D warnings
  else
    echo "warning: x86_64-pc-windows-gnu not installed: Windows type-check skipped (rustup target add x86_64-pc-windows-gnu)"
  fi
else
  echo "No Cargo workspace yet: Rust checks skipped."
fi
# ADR 0001: the core crate stays GPUI-free, directly or through any dependency.
for m in crates/*core*/Cargo.toml; do
  [ -f "$m" ] || continue
  if cargo tree --manifest-path "$m" --prefix none | grep -qi '^gpui'; then
    echo "GPUI dependency in the core crate ($m)"
    exit 1
  fi
done
# CONVENTIONS.md stays small enough to read in full.
if [ "$(wc -c < docs/CONVENTIONS.md)" -gt 8192 ]; then
  echo "docs/CONVENTIONS.md is over 8 KB: prune it"
  exit 1
fi
echo "CI checks passed."
