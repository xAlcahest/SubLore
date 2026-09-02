#!/bin/bash
# Run one behavioural check under its own X server, stream its output while it runs, keep all of it
# for the artefact, and record what the check itself returned.
#
# The exit code is what the verdict is built on. Reading the log text instead was guesswork about
# the shape of somebody else's output: it could not tell a check that failed from one that never
# ran, and a WebdriverIO run that executed no specs at all read as green.
set -u

if [ "$#" -ne 2 ]; then
  echo "usage: e2e-check.sh <name> <pnpm script>" >&2
  exit 64
fi
name="$1"
script="$2"

mkdir -p ci-logs
log="ci-logs/$name.log"
: > "$log"

# Says whether a quiet check is alive, which is the question a run that looks stopped is asking.
# Only after 20s without a write to the log, then every 15s, so a talking check costs nothing.
heartbeat() {
  local mtime quiet said=0
  while sleep 5; do
    mtime=$(stat -c %Y "$log" 2>/dev/null) || return
    quiet=$(($(date +%s) - mtime))
    if [ "$quiet" -lt 20 ]; then
      said=0
    elif [ "$said" -eq 0 ] || [ "$quiet" -ge $((said + 15)) ]; then
      said=$quiet
      printf '%s: %dm%02ds elapsed, no output for %ds\n' \
        "$name" $((SECONDS / 60)) $((SECONDS % 60)) "$quiet"
    fi
  done
}

SECONDS=0
# The screen is sized explicitly: a root window smaller than the window under test fails the
# fixture, and the xvfb-run default differs per distribution. 1920x1080 is the largest size a check
# can resize the app window to and still have all of it on screen.
# Still a regular file, never a pipe: through a pipe node drops whatever is still buffered when a
# runner calls process.exit(), and that is the end of a failing check's output.
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm "$script" < /dev/null > "$log" 2>&1 &
check=$!

# Follow the file instead of standing between the check and it, so the check's own fds never
# change. --pid ends the follower with the check, after one last read.
tail -n +1 -f -s 0.2 --pid="$check" "$log" &
follow=$!
heartbeat &
beat=$!

wait "$check"
status=$?
kill "$beat" 2>/dev/null
wait "$follow"
wait "$beat"

# A check killed mid-line leaves the log open, and the line below would weld onto it. `od` rather
# than a command substitution on the byte: substitution silently drops a NUL and reports no newline.
if [ -s "$log" ] && [ "$(tail -c1 "$log" | od -An -tx1 | tr -d ' \n')" != "0a" ]; then
  echo
fi
printf '%s: exit %s after %dm%02ds\n' "$name" "$status" $((SECONDS / 60)) $((SECONDS % 60))
echo "$status" > "ci-logs/$name.exit"
exit "$status"
