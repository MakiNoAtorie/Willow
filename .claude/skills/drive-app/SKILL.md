---
name: drive-app
description: "Drive the running Willow app on macOS and screenshot its window. Use for a ticket's hand check (macOS), to see whether an overlay, menu or popup actually shows, or to confirm a UI change in the real app rather than in core tests."
---

`drive.sh` builds the release app, launches it on `--generate 5` (override with `WILLOW_ARGS`), plays the steps, then captures **only the Willow window** and closes the app:

```bash
.claude/skills/drive-app/drive.sh OUT.png enter cmd+shift+c right
.claude/skills/drive-app/drive.sh OUT.png rclick:590,370
```

Put `OUT.png` in the scratchpad, then Read it. The app's stderr lands in `OUT.log`, so temporary `eprintln!` lines are readable there.

Steps, run in order 0.3 s apart:
- A key: `enter`, `esc`, `tab`, `delete`, `left`/`right`/`up`/`down`, or one character, with `cmd+`, `ctrl+`, `shift+` or `alt+` prefixes (`cmd+shift+down`).
- `type:TEXT` types text. `sleep:S` waits.
- `click:X,Y` and `rclick:X,Y` click at window-relative points. The script prints the window size, and `--generate 5` centers its first shape near `590,370`.

The script takes over the user's keyboard and mouse focus for a few seconds. So run it only from the main session, one run at a time, and say so before the first run in a session. Screenshot the window only (the script's `-R` region), never the full screen: the rest of the desktop is the user's.

A screenshot that misses something the log says was drawn is a real rendering bug, not a capture limit. Phase 4's menus were laid out off-canvas for a whole phase that way.
