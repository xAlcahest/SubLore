#!/bin/sh
# Regenerates sample.mkv: synthetic 60s test clip (video counter + 440Hz tone). Not committed; see .gitignore.
set -e
cd "$(dirname "$0")"
ffmpeg -y -f lavfi -i "testsrc2=duration=60:size=640x360:rate=30" \
  -f lavfi -i "sine=frequency=440:duration=60" \
  -c:v libx264 -preset fast -crf 28 -pix_fmt yuv420p -c:a aac -b:a 64k \
  -shortest sample.mkv
