#!/bin/bash
# M0.3: install the .deb on a clean image, start what it installed, then start the AppImage.
#
# The verdict is this script's exit status, for the reason .github/scripts/e2e-check.sh gives: a
# verdict built on the shape of somebody else's output cannot tell a check that failed from one
# that never ran.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

# Both artifacts are named on purpose. An optional argument is how a smoke ends up checking half of
# what it says it checks, in silence.
if [ $# -ne 2 ]; then
  echo "usage: $0 <path to the .deb> <path to the .AppImage>" >&2
  exit 2
fi
deb=$(realpath "$1")
appimage=$(realpath "$2")

for artifact in "$deb" "$appimage"; do
  if [ ! -s "$artifact" ]; then
    echo "packaging smoke: $artifact does not exist or is empty" >&2
    exit 2
  fi
done

# This installs a package system-wide and rewrites apt's state. It belongs in a throwaway image,
# never on somebody's machine.
if [ "$(id -u)" != "0" ]; then
  echo "packaging smoke: run this as root in a throwaway container, e.g. ubuntu:24.04" >&2
  exit 2
fi

# Where the .deb puts the binary. Written out rather than looked up: a package that stops shipping
# it has to fail here, not adapt to whatever it shipped instead.
installed=/usr/bin/sublore

export DEBIAN_FRONTEND=noninteractive
apt-get update
# xvfb and x11-utils give the app a display and let the check read the window off it; nodejs runs
# the check; libopengl0 is the one library linuxdeploy leaves to the host, and a bare image has no
# host to leave it to. --no-install-recommends keeps the image bare, which is the point of using one.
apt-get install -y --no-install-recommends libopengl0 nodejs xauth x11-utils xvfb

echo "== installing $deb"
# Resolving its dependencies is half of this check: a library the package never declared is not
# here, and the launch below is where that shows up.
apt-get install -y --no-install-recommends "$deb"
dpkg -L sublore

# Same screen size and the same reason as .github/scripts/e2e-check.sh: a root window smaller than
# the app window fails the fixture, and the xvfb-run default differs per distribution.
echo "== starting $installed"
xvfb-run -a -s "-screen 0 1920x1080x24" node "$here/package-smoke.mjs" "$installed"

# After the .deb, deliberately: linuxdeploy leaves the base X and GL stack to the host, so a bare
# image would fail the AppImage for the image's reasons rather than the bundle's. The mode bit is
# set here because the artifact arrives through a zip, which does not carry one.
echo "== starting $appimage"
chmod +x "$appimage"
# A container has no FUSE, and this is the AppImage's own supported way of running without it.
APPIMAGE_EXTRACT_AND_RUN=1 \
  xvfb-run -a -s "-screen 0 1920x1080x24" node "$here/package-smoke.mjs" "$appimage"
