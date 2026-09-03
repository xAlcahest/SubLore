#!/bin/bash
# Every behavioural check that should have run, and what it returned.
#
# The list lives here rather than being derived from what is on disk, because a check that never ran
# leaves nothing on disk: reading the directory reports six green logs and says nothing about the
# seventh. A name on disk that is not in this list fails too, so a step and this list cannot drift
# apart in silence. Adding a check means adding its name here.
set -u
EXPECTED="smoke shutdown close-gate late-edit quit-gate startup-args mpv-context picker-thread waveform-budget scale no-display"

missing=""
failed=""
for name in $EXPECTED; do
  if [ ! -f "ci-logs/$name.exit" ]; then
    missing="$missing $name"
    continue
  fi
  status=$(cat "ci-logs/$name.exit")
  if [ "$status" != "0" ]; then
    failed="$failed $name(exit $status)"
  fi
done

unexpected=""
for file in ci-logs/*.exit; do
  [ -e "$file" ] || continue
  name=$(basename "$file" .exit)
  case " $EXPECTED " in
    *" $name "*) ;;
    *) unexpected="$unexpected $name" ;;
  esac
done

if [ -n "$missing" ]; then
  echo "checks that never reported:$missing"
fi
if [ -n "$failed" ]; then
  echo "checks that failed:$failed"
fi
if [ -n "$unexpected" ]; then
  echo "checks that ran but are not in the expected list:$unexpected"
  echo "add them to EXPECTED in .github/scripts/e2e-verdict.sh, or they are not being checked"
fi
if [ -n "$missing$failed$unexpected" ]; then
  echo "the diagnostic logs are in the e2e-diagnostic-logs artefact"
  exit 1
fi

echo "every check ran and returned zero:$(printf ' %s' $EXPECTED)"
