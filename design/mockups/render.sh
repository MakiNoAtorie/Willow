#!/bin/sh
# Render every mockup screen to png/<theme>/<screen>.png (1280x820) with headless Chrome.
# Usage: ./render.sh [theme] [os]   (defaults: paper, each screen's own platform)
cd "$(dirname "$0")"
theme=${1:-paper}; os=${2:-}
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
mkdir -p "png/$theme"
for id in $(grep -o "{ id: '[a-z-]*', title:" mockups.html | cut -d"'" -f2); do
  out="png/$theme/$id${os:+-$os}.png"
  "$chrome" --headless=new --disable-gpu --hide-scrollbars --window-size=1280,820 --virtual-time-budget=6000 \
    --screenshot="$PWD/$out" "file://$PWD/mockups.html?screen=$id&theme=$theme${os:+&os=$os}&shot" >/dev/null 2>&1 && echo "$out"
done
