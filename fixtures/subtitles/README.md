# Subtitle fixtures

Real-shaped subtitle files, committed to the repo and treated as byte-exact test data. Unlike the
video fixture, these are small, hand-authored and never generated: the bytes themselves are the test.

## Layout

```
srt/clean/*.srt          must round-trip parse -> serialize byte for byte
srt/malformed/*.srt      must fail with the error named in its sidecar
srt/malformed/*.srt.expected
vtt/clean/*.vtt          vtt/malformed/*.vtt + *.vtt.expected
ass/clean/*.ass          ass/clean/*.ssa   ass/malformed/*.ass + *.ass.expected
```

## Sidecar format

One line, next to the malformed fixture it describes:

```
7:BadTimecode:the end timestamp has a letter in it
```

The line number and the error kind are asserted. The trailing note is for humans. A clean fixture
has no sidecar; a sidecar without a fixture, or a malformed fixture without a sidecar, fails the
test harness.

## Rules

- `.gitattributes` marks this tree `-text`, so git never rewrites a line ending here. Do not remove
  that rule: on Windows, `core.autocrlf=true` would mangle every CRLF fixture and the round-trip
  tests would pass on the wrong bytes.
- `.prettierignore` covers this tree for the same reason.
- Write files that look like they came off a real disk: real dialogue, real timings, real mess. No
  `Lorem ipsum`, no `cue 1 / cue 2 / cue 3` unless the fixture is about numbering itself.
- Every fixed bug gets a regression fixture here (CLAUDE.md §5.3).
- A new tolerance needs a fixture and a BACKLOG entry before the parser learns it.

## Adding a fixture

1. Drop the file in `clean/` or `malformed/` under its format.
2. For a malformed one, write the `.expected` sidecar next to it.
3. Add its semantic expectations to that format's test file (cue counts, exact timecodes, the kind
   sequence): byte-identity alone does not prove the parser understood anything.
4. Raise the `MIN_CLEAN` / `MIN_MALFORMED` guard in that test file if the tree grew past it.
