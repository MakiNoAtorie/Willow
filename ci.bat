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
rem ADR 0001: the core crate stays GPUI-free, directly or through any dependency.
for /d %%d in (crates\*core*) do if exist "%%d\Cargo.toml" (
  cargo tree --manifest-path "%%d\Cargo.toml" --prefix none | findstr /b /i "gpui" >nul && (echo GPUI dependency in the core crate ^(%%d^) & exit /b 1)
)
rem CONVENTIONS.md stays small enough to read in full.
for %%A in (docs\CONVENTIONS.md) do if %%~zA GTR 8192 (echo docs\CONVENTIONS.md is over 8 KB: prune it & exit /b 1)
echo CI checks passed.
