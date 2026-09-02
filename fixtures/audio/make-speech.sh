#!/bin/sh
# Generate the speech fixture the real-binary ASR tests transcribe. See BACKLOG.md M3.1.
#
# Generated, never committed: no voice audio enters the repo, only the recipe (CONTRIBUTING.md §8).
# The text is deliberately ordinary English with words tiny.en gets right.
set -e

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

command -v espeak-ng >/dev/null 2>&1 || {
	echo "make-speech: espeak-ng not found (Fedora: sudo dnf install espeak-ng; Debian: apt install espeak-ng)" >&2
	exit 1
}
command -v ffmpeg >/dev/null 2>&1 || {
	echo "make-speech: ffmpeg not found (see README.md)" >&2
	exit 1
}

mkdir -p fixtures/audio
# Through a file rather than a pipe: this is `#!/bin/sh`, where `set -e` reads only the last status
# in a pipeline, so espeak-ng dying part way through would hand ffmpeg a shorter recording and
# ffmpeg would encode it happily. The fixture would be quietly too short.
raw=$(mktemp)
trap 'rm -f "$raw"' EXIT
espeak-ng -v en-us -s 145 -p 40 --stdout \
	"Sublore keeps your terminology consistent across every episode. The translator opens a subtitle file, and the memory follows." > "$raw"
ffmpeg -y -hide_banner -loglevel error -i "$raw" -ac 1 -ar 16000 -c:a pcm_s16le fixtures/audio/speech-en.wav

echo "make-speech: wrote fixtures/audio/speech-en.wav"
ffprobe -hide_banner -loglevel error -show_entries format=duration,size -of default=noprint_wrappers=1 fixtures/audio/speech-en.wav
