#!/bin/sh
# Build the whisper.cpp sidecar binaries Sublore spawns. See BACKLOG.md M3.1.
#
# Two binaries come out of one pinned commit: whisper-cli (Vulkan) and whisper-cli-cpu.
# The CPU one has no Vulkan loader dependency at all, which is what makes CLAUDE.md §2's
# "CPU fallback always working" a property of the shipped files instead of the user's drivers.
# Everything lands in .whisper/, which is git-ignored: no binary ever enters the repo.
set -e

usage() {
	echo "usage: sh scripts/build-whisper.sh [--cpu-only] [--jobs N]" >&2
	exit 2
}

fail() {
	echo "build-whisper: $1" >&2
	exit 1
}

cpu_only=0
jobs=""
while [ $# -gt 0 ]; do
	case "$1" in
	--cpu-only) cpu_only=1 ;;
	--jobs)
		shift
		[ $# -gt 0 ] || usage
		jobs="$1"
		;;
	-h | --help) usage ;;
	*) usage ;;
	esac
	shift
done

repo_root=$(git rev-parse --show-toplevel) || fail "not inside a git work tree"
cd "$repo_root" || fail "cannot enter repo root '$repo_root'"

[ -f whisper.pin ] || fail "whisper.pin is missing"
repo=$(sed -n 's/^repo=//p' whisper.pin | head -n 1)
commit=$(sed -n 's/^commit=//p' whisper.pin | head -n 1)
[ -n "$repo" ] || fail "whisper.pin has no repo= line"
# A short or non-hex commit would silently check out something else.
echo "$commit" | grep -qE '^[0-9a-f]{40}$' || fail "whisper.pin commit is not a 40-character sha"

for tool in git cmake; do
	command -v "$tool" >/dev/null 2>&1 || fail "$tool not found in PATH (Fedora: sudo dnf install $tool)"
done
if [ "$cpu_only" -eq 0 ]; then
	command -v glslc >/dev/null 2>&1 ||
		fail "glslc not found: the Vulkan build needs it (Fedora: sudo dnf install glslc; Debian: glslang-tools; Windows: LunarG Vulkan SDK). Use --cpu-only to skip the Vulkan build."
fi
[ -n "$jobs" ] || jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)

src=.whisper/src
mkdir -p "$src" .whisper/bin

if [ ! -d "$src/.git" ]; then
	echo "build-whisper: cloning $repo"
	git init -q "$src"
	git -C "$src" remote add origin "$repo"
fi
git -C "$src" remote set-url origin "$repo"
if ! git -C "$src" cat-file -e "$commit^{commit}" 2>/dev/null; then
	echo "build-whisper: fetching $commit"
	git -C "$src" fetch --depth 1 origin "$commit" ||
		fail "could not fetch $commit from $repo (no network, or the commit was removed upstream)"
fi
git -C "$src" checkout -q --force --detach "$commit"
# Anything left from an earlier build of another commit would be linked in silently.
git -C "$src" clean -qfdx -e build-cpu -e build-vulkan

build() {
	name=$1
	vulkan=$2
	dir=".whisper/$name"
	echo "build-whisper: configuring $name (GGML_VULKAN=$vulkan)"
	# GGML_NATIVE=OFF on purpose: -march=native produces a binary that crashes on any machine
	# older than the one that built it, which is not something to ship or to compare runs across.
	cmake -S "$src" -B "$dir" \
		-DCMAKE_BUILD_TYPE=Release \
		-DBUILD_SHARED_LIBS=OFF \
		-DGGML_NATIVE=OFF \
		-DGGML_VULKAN="$vulkan" \
		-DWHISPER_BUILD_EXAMPLES=ON \
		-DWHISPER_BUILD_TESTS=OFF \
		-DWHISPER_BUILD_SERVER=OFF >"$dir.log" 2>&1 ||
		{
			tail -n 20 "$dir.log" >&2
			fail "cmake configure failed for $name (full log: $dir.log)"
		}
	echo "build-whisper: building $name with $jobs jobs"
	cmake --build "$dir" --config Release --target whisper-cli -j "$jobs" >>"$dir.log" 2>&1 ||
		{
			tail -n 20 "$dir.log" >&2
			fail "build failed for $name (full log: $dir.log)"
		}
}

install_binary() {
	dir=$1
	target=$2
	for candidate in "$dir/bin/whisper-cli" "$dir/bin/Release/whisper-cli.exe" "$dir/bin/whisper-cli.exe"; do
		if [ -f "$candidate" ]; then
			case "$candidate" in
			*.exe) cp -f "$candidate" ".whisper/bin/$target.exe" && return 0 ;;
			*) cp -f "$candidate" ".whisper/bin/$target" && return 0 ;;
			esac
		fi
	done
	fail "no whisper-cli produced under $dir/bin"
}

build build-cpu OFF
install_binary .whisper/build-cpu whisper-cli-cpu
if [ "$cpu_only" -eq 0 ]; then
	build build-vulkan ON
	install_binary .whisper/build-vulkan whisper-cli
fi

echo "build-whisper: done, from commit $commit"
ls -l .whisper/bin
echo "build-whisper: point SUBLORE_WHISPER_BIN at one of these, or leave it unset and the app will find .whisper/bin itself."
