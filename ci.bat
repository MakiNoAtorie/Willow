@echo off
rem Runs the CI checks locally. Pass the cargo job count (default 4; use 2 when another build runs).
cd /d "%~dp0"
set J=%1
if "%J%"=="" set J=4
if not exist Cargo.toml (echo No Cargo workspace yet: Rust checks skipped. & goto rules)
cargo fmt --all --check || exit /b 1
cargo build -j %J% --workspace --all-targets || exit /b 1
cargo test -j %J% --workspace -- --test-threads=2 || exit /b 1
cargo clippy -j %J% --workspace --all-targets -- -D warnings || exit /b 1
:rules
rem ADR 0001: the core crate stays GPUI-free.
git grep -q -i "gpui" -- "crates/*core*/Cargo.toml" && (echo GPUI dependency in the core crate & exit /b 1)
echo CI checks passed.
