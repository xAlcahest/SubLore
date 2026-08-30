# Gate 2 — reference battery, 2026-08-30

Run at Wave 0, on `GATE_HEAD=eca9806`, before any lens read anything. A lens reporting that the suite is red is measured against this, not against memory.

```
=== rust ===
fmt: clean
clippy: exit 0
cargo test: 502 passed, 0 failed
=== frontend ===
eslint: clean
=== rebuild, after cargo ===
binary: rebuilt
=== behavioural ===
wdio: Spec Files:	 8 passed, 8 total (100% completed) in 00:00:59
shutdown check passed (5/5 checks)
close gate check passed (12/12 checks)
scaled surface check passed (5/5 checks)
wayland attach check passed (4/4 checks)
wayland attach check passed (4/4 checks)
wayland attach check passed (4/4 checks)
```

**One line is missing from that block and it is worth saying why.** `prettier --check` ran while this
very file was being written by the same redirection, so it saw a half-written document and reported
it. Re-run afterwards against the finished tree it is clean. Nothing in the suite was red at
`GATE_HEAD`.
