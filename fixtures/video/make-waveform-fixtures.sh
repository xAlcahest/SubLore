#!/bin/sh
# Regenerates the M2.4 waveform fixtures: media whose audio is known, so a peak read out of one
# can be asserted against a number instead of against itself. Generated, never committed, the way
# sample.mkv is (BACKLOG.md M2.4, CONTRIBUTING.md §8, .gitignore).
#
# Usage: sh fixtures/video/make-waveform-fixtures.sh [--with-24min]
#
# Audio is 48 kHz mono FLAC everywhere: FLAC is lossless, so the samples a test decodes are exactly
# the ones written here, and 48 whole samples fall inside every millisecond bucket. Video is
# testsrc2 at 640x360 and 30 fps, the picture make-sample.sh already writes; nothing asserts
# anything about it. Full scale below is a 16-bit sample at its limit: 32767 up, -32768 down, so
# a peak magnitude reads 32768 as often as 32767.
#
# What a test may assert about each file:
#
# waveform-60s.mkv     60 s, one audio track, six 10 s blocks alternating a full-scale 440 Hz tone
#                      and digital silence, tone first: tone [0,10), silence [10,20), tone [20,30),
#                      silence [30,40), tone [40,50), silence [50,60). Every silence sample is
#                      exactly 0, and every block edge is an exact sample boundary. A millisecond
#                      bucket is shorter than a 440 Hz cycle, so a tone bucket's peak is not always
#                      full scale: measured over this file every tone bucket lands between 32188
#                      and 32768, the smallest being 98.2% of full scale, and that is what a
#                      tolerance on "the tone reads full" has to cover.
# waveform-tracks.mkv  30 s, two audio tracks, both unbroken tones: track 1 (jpn) 440 Hz at full
#                      scale, track 2 (eng) 880 Hz at a quarter of it. The amplitudes differ, so a
#                      check can tell which track was read without listening to anything. 880 Hz
#                      fits inside a millisecond bucket, so track 2's bucket peaks all land between
#                      8181 and 8192, a quarter of track 1's.
# waveform-silent.mkv  20 s of video with no audio stream at all. Not a silent track: ffprobe
#                      reports zero audio streams.
# waveform-24min.mkv   1440 s, one audio track, waveform-60s.mkv's pattern continued to 144 blocks,
#                      tone first. Only written with --with-24min and never in CI: it is the
#                      fixture for the 24-minute number, which is measured on the owner's machine.
#
# Reruns overwrite without prompting and produce the same durations, stream counts and samples.
# The bytes are not the same: Matroska stamps a segment id and the encoders have not been checked
# for determinism, and nothing here needs them to be.
set -e

cd "$(dirname "$0")"

for tool in ffmpeg ffprobe; do
	command -v "$tool" >/dev/null 2>&1 || {
		echo "make-waveform-fixtures: $tool not found (see README.md)" >&2
		exit 1
	}
done

with_24min=no
for arg in "$@"; do
	case "$arg" in
	--with-24min) with_24min=yes ;;
	*)
		echo "make-waveform-fixtures: unknown argument '$arg' (usage: [--with-24min])" >&2
		exit 1
		;;
	esac
done

# Tone for ten seconds, silence for ten, from t alone: the block a sample belongs to is
# floor(t/10), and the odd ones are multiplied away. The phase is continuous through t and every
# block boundary is a whole number of 440 Hz cycles, so each tone block starts at a zero crossing.
blocks='sin(2*PI*440*t)*(1-mod(floor(t/10)\,2))'

video='testsrc2=size=640x360:rate=30:duration'

ffmpeg -y -hide_banner -loglevel error \
	-f lavfi -i "$video=60" \
	-f lavfi -i "aevalsrc=exprs=$blocks:duration=60:sample_rate=48000:channel_layout=mono" \
	-c:v libx264 -preset fast -crf 28 -pix_fmt yuv420p -c:a flac -sample_fmt s16 \
	waveform-60s.mkv

ffmpeg -y -hide_banner -loglevel error \
	-f lavfi -i "$video=30" \
	-f lavfi -i "aevalsrc=exprs=sin(2*PI*440*t):duration=30:sample_rate=48000:channel_layout=mono" \
	-f lavfi -i "aevalsrc=exprs=sin(2*PI*880*t)*0.25:duration=30:sample_rate=48000:channel_layout=mono" \
	-map 0:v -map 1:a -map 2:a \
	-metadata:s:a:0 language=jpn -metadata:s:a:0 title="Japanese original" \
	-metadata:s:a:1 language=eng -metadata:s:a:1 title="English dub" \
	-c:v libx264 -preset fast -crf 28 -pix_fmt yuv420p -c:a flac -sample_fmt s16 \
	waveform-tracks.mkv

ffmpeg -y -hide_banner -loglevel error \
	-f lavfi -i "$video=20" \
	-an -c:v libx264 -preset fast -crf 28 -pix_fmt yuv420p \
	waveform-silent.mkv

written='waveform-60s.mkv waveform-tracks.mkv waveform-silent.mkv'

if [ "$with_24min" = yes ]; then
	ffmpeg -y -hide_banner -loglevel error \
		-f lavfi -i "$video=1440" \
		-f lavfi -i "aevalsrc=exprs=$blocks:duration=1440:sample_rate=48000:channel_layout=mono" \
		-c:v libx264 -preset fast -crf 28 -pix_fmt yuv420p -c:a flac -sample_fmt s16 \
		waveform-24min.mkv
	written="$written waveform-24min.mkv"
fi

for file in $written; do
	echo "make-waveform-fixtures: wrote fixtures/video/$file"
	ffprobe -hide_banner -loglevel error \
		-show_entries format=duration \
		-show_entries stream=index,codec_type,codec_name,sample_rate,channel_layout \
		-show_entries stream_tags=language,title \
		-of default=noprint_wrappers=1 "$file"
done
