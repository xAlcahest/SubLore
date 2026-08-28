#!/bin/sh
# Fetch the whisper model the E2E harness runs the app against, into the same cache directory the
# gated real-asr suite uses. See BACKLOG.md M3.2.
#
# Explicit and manual: nothing inside Sublore downloads on its own (CLAUDE.md §1), and the model is
# never committed (CLAUDE.md §8). The two values below are ggml-tiny.en.bin's row in
# crates/sublore-asr/src/model/catalog.rs; if that row ever changes, change them here too.
set -e

file=ggml-tiny.en.bin
sha256=921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f
url="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$file"

dir=${SUBLORE_TEST_MODEL_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sublore/models}
target="$dir/$file"

verify() {
	[ -f "$1" ] && printf '%s  %s\n' "$sha256" "$1" | sha256sum -c - >/dev/null 2>&1
}

if verify "$target"; then
	echo "fetch-model: $target is already here and hashes to its catalogue row"
	exit 0
fi

command -v curl >/dev/null 2>&1 || {
	echo "fetch-model: curl not found (Fedora: sudo dnf install curl; Debian: apt install curl)" >&2
	exit 1
}
command -v sha256sum >/dev/null 2>&1 || {
	echo "fetch-model: sha256sum not found (it comes with coreutils)" >&2
	exit 1
}

mkdir -p "$dir"
echo "fetch-model: downloading $url"
curl -fL --retry 3 --progress-bar --output "$target.part" "$url"

# The final name is only taken once the checksum agrees, which is the rule the app's own download
# follows: a corrupt file never gets the name a run would pick up.
if ! verify "$target.part"; then
	rm -f "$target.part"
	echo "fetch-model: the download does not hash to $sha256; nothing was installed" >&2
	exit 1
fi
mv "$target.part" "$target"
echo "fetch-model: wrote $target"
