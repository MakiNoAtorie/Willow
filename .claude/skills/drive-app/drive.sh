#!/bin/sh
# Launches Willow, plays steps, screenshots only its window. See SKILL.md.
# Usage: drive.sh OUT.png STEP...   (stderr of the app goes to OUT.log)
set -e
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
out=$1; shift
cache=${TMPDIR:-/tmp}/willow-drive
mkdir -p "$cache"
[ "$cache/click" -nt "$here/click.swift" ] || swiftc -O "$here/click.swift" -o "$cache/click"
if (cd "$repo" && cargo build -q --release -p willow-app) 2>&1 | grep -E '^error'; then exit 1; fi
# Preferences and recovery files land here, never in the user's own app data.
[ -n "$WILLOW_KEEP_HOME" ] || rm -rf "$cache/home"
mkdir -p "$cache/home"
HOME=$cache/home "$repo/target/release/willow" ${WILLOW_ARGS---generate 5} >"${out%.png}.log" 2>&1 &
pid=$!
trap 'kill $pid 2>/dev/null' EXIT
sleep 3
osa() { osascript -e "tell application \"System Events\" to $1"; }
osa 'set frontmost of (first process whose unix id is '"$pid"') to true'
sleep 0.5
bounds=$(osa 'tell (first process whose unix id is '"$pid"') to get {position, size} of window 1' | tr -d ' ')
IFS=, read -r wx wy ww wh <<B
$bounds
B
for step in "$@"; do
  case $step in
    sleep:*) sleep "${step#sleep:}" ;;
    type:*) osa "keystroke \"${step#type:}\"" ;;
    open:*)
      osa 'keystroke "o" using {command down}'; sleep 1
      osa 'keystroke "g" using {command down, shift down}'; sleep 0.5
      osa "keystroke \"${step#open:}\""; sleep 0.5
      osa 'key code 36'; sleep 0.8
      osa 'key code 36'; sleep 1.5 ;;
    click:*|rclick:*|dclick:*|move:*|down:*|drag:*|up:*)
      xy=${step#*:}; x=$((wx + ${xy%,*})); y=$((wy + ${xy#*,}))
      case ${step%%:*} in click) b=left ;; rclick) b=right ;; dclick) b=double ;; *) b=${step%%:*} ;; esac
      "$cache/click" "$b" "$x" "$y" ;;
    *)
      # key: [cmd+][ctrl+][shift+][alt+]NAME, NAME a key below or one character.
      mods=""; k=$step
      while :; do
        case $k in
          cmd+*) mods="$mods, command down"; k=${k#cmd+} ;;
          ctrl+*) mods="$mods, control down"; k=${k#ctrl+} ;;
          shift+*) mods="$mods, shift down"; k=${k#shift+} ;;
          alt+*) mods="$mods, option down"; k=${k#alt+} ;;
          *) break ;;
        esac
      done
      using=""; [ -n "$mods" ] && using=" using {${mods#, }}"
      case $k in
        enter) osa "key code 36$using" ;; esc) osa "key code 53$using" ;;
        tab) osa "key code 48$using" ;; delete) osa "key code 51$using" ;;
        left) osa "key code 123$using" ;; right) osa "key code 124$using" ;;
        down) osa "key code 125$using" ;; up) osa "key code 126$using" ;;
        *) osa "keystroke \"$k\"$using" ;;
      esac ;;
  esac
  sleep 0.3
done
sleep 0.5
screencapture -x -C -R"$wx,$wy,$ww,$wh" "$out"
# One image pixel per window point, so screenshot coordinates are click coordinates.
sips -Z "$((ww > wh ? ww : wh))" "$out" >/dev/null
echo "$out (window at $wx,$wy ${ww}x$wh points)"
