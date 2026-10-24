@echo off
rem Build and run the GPUI spike. Extra args pass through, e.g. run.bat --bench --machine win-9600x-igpu
cd /d "%~dp0.."
cargo run --release -p spike-gpui -- %*
