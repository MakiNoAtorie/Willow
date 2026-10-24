#!/bin/sh
# Build and run the GPUI spike (macOS twin of run.bat). Extra args pass through, e.g. ./run.sh --large
cd "$(dirname "$0")/.."
exec cargo run --release -p spike-gpui -- "$@"
