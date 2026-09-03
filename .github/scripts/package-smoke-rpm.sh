#!/bin/bash
# M0.3 for the rpm: install it on a clean Fedora image, resolve its dependencies there, and start
# the binary it installed. The bundler declares an rpm's dependencies from a list in the config
# rather than from the binary, so a library nobody wrote down is only ever found by a launch.
#
# The verdict is this script's exit status, for the reason .github/scripts/e2e-check.sh gives: a
# verdict built on the shape of somebody else's output cannot tell a check that failed from one
# that never ran.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

if [ $# -ne 1 ]; then
  echo "usage: $0 <path to the .rpm>" >&2
  exit 2
fi
package=$(realpath "$1")

if [ ! -s "$package" ]; then
  echo "packaging smoke: $package does not exist or is empty" >&2
  exit 2
fi

# This installs a package system-wide and rewrites dnf's state. It belongs in a throwaway image,
# never on somebody's machine.
if [ "$(id -u)" != "0" ]; then
  echo "packaging smoke: run this as root in a throwaway container, e.g. fedora:latest" >&2
  exit 2
fi

# Where the rpm puts the binary. Written out rather than looked up: a package that stops shipping
# it has to fail here, not adapt to whatever it shipped instead.
installed=/usr/bin/sublore

# xvfb and xwininfo give the app a display and let the check read the window off it, procps-ng is
# what the harness tears the process group down with, and nodejs runs the check. Weak dependencies
# are off so the image stays bare, which is the point of using one.
dnf install -y --setopt=install_weak_deps=False \
  nodejs procps-ng xorg-x11-server-Xvfb xorg-x11-xauth xwininfo

echo "== installing $package"
# Resolving its dependencies is half of this check: dnf refuses a Requires nothing here provides,
# and a library the package never declared shows up at the launch below instead.
dnf install -y --setopt=install_weak_deps=False "$package"
rpm -ql sublore

# Same screen size and the same reason as .github/scripts/e2e-check.sh: a root window smaller than
# the app window fails the fixture, and the xvfb-run default differs per distribution.
echo "== starting $installed"
xvfb-run -a -s "-screen 0 1920x1080x24" node "$here/package-smoke.mjs" "$installed"
