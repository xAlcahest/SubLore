#!/bin/bash
# Run one behavioural check under its own X server, keep its output, and record what it returned.
#
# The exit code is what the verdict is built on. Reading the log text instead was guesswork about
# the shape of somebody else's output: it could not tell a check that failed from one that never
# ran, and a WebdriverIO run that executed no specs at all read as green.
set -u
name="$1"
script="$2"

mkdir -p ci-logs
# The screen is sized explicitly: a root window smaller than the window under test fails the
# fixture, and the xvfb-run default differs per distribution. 1920x1080 is the largest size a check
# can resize the app window to and still have all of it on screen.
# No pipe to tee: with a pipeline the status belongs to tee unless pipefail is on, and this must be
# the check's own status whatever the shell is configured to do.
xvfb-run -a -s "-screen 0 1920x1080x24" pnpm "$script" > "ci-logs/$name.log" 2>&1
status=$?
cat "ci-logs/$name.log"
echo "$status" > "ci-logs/$name.exit"
exit "$status"
