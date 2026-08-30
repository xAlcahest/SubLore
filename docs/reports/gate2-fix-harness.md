# Gate 2, Wave 3 — the e2e harness cluster

Files owned: `e2e/scripts/real-session-check.mjs`, `e2e/scripts/close-gate-check.js`,
`e2e/scripts/wayland-attach-check.js`, `e2e/scripts/scaled-surface-check.js`,
`e2e/scripts/n1b-load-probe.js`. One new file was added, `e2e/lib/gtk-dialog.js`, to close a
duplication row that spans two owned scripts; no other file was touched.

Verification method: `node --check` on every edited file, `npx prettier --check`/`--write`, and
`npx eslint`, then a real run of every changed script. `wayland-attach-check.js` and the rewritten
`real-session-check.mjs` were run for real against the live desktop — both are argv/launch/capture
only after their fixes, which is exactly what WORKFLOW.md 4c permits there. `close-gate-check.js`,
`scaled-surface-check.js` and `n1b-load-probe.js` all send real synthetic input, so each was run
against a throwaway Xvfb I started myself on an unused display (never `:0`, the owner's live
session), per 4c and per §5's build-order rule (the binary was already fresh from `pnpm e2e:build`;
no `cargo test`/`clippy` was run in this session, so no rebuild was needed before behavioural runs).

## Assertions that cannot fail (the three-site row)

`scaled-surface-check.js:148`, `close-gate-check.js:261/283/322`, `wayland-attach-check.js:112` —
**fixed**, all four sites (`close-gate-check.js` really did have three, as the row warned).

Each was `check(label, x !== null)` immediately after an `await waitFor(...)` whose own contract
(`e2e/lib/proc.js`) is to return a truthy value or throw — so the null case was already unreachable.
Per the register's instruction, each was replaced with a live assertion rather than folded into the
counter: `mapState(id) === "IsViewable"` on the same window, which `xwininfo -tree` does not
guarantee (a window can be a child in the tree while unmapped) and which the code did not check
before. `wayland-attach-check.js` already used exactly this pattern one check later for the surface;
the fix applies the same pattern to the toplevel, the dialog (three times), and the scaled surface.

`scaled-surface-check.js` needed one extra step: the window closes before `main()` asserts, so
`measureAt()` now captures `mapState(surface.id)` while the window is still alive and returns it,
rather than the check trying to query a window that no longer exists.

Proof: `pnpm e2e:close-gate` (12/12), `pnpm e2e:wayland` (4/4), and `scaled-surface-check.js` (5/5)
all pass with the new assertions actually executing and printing `ok`, run twice each (once before,
once after a `prettier --write` reformat) under an isolated Xvfb or the real Wayland session as
appropriate. Removing any of the four `mapState` checks — verified by reading, not by breaking them
on purpose and re-running, to avoid burning more real-hardware app launches than necessary — would
now drop a check that can genuinely fail, unlike the ones they replaced.

## `real-session-check.mjs` — decided: fix it into a declared probe, obeying 4c in full

**Fixed.** The file is rewritten. What changed and why:

- **Declared its own category**, in its header, in the same words the repo's other undeclared-until-now
  probe uses: `**This is a probe, not a check. It asserts nothing.**` (matching
  `n1b-load-probe.js:4`, which L9's own report treats as the sound precedent). It states plainly why
  it has no `pnpm` script entry and does not run in CI: it needs the owner's own Wayland session,
  the same reason `wayland-attach-check.js` gives for its own CI exclusion.
- **Stopped sending input to the real display.** The old script launched with empty argv, then typed
  the fixture path and clicked Open on the owner's live desktop — the exact incident WORKFLOW 4c was
  written after, in the video sibling script this same commit fixed and this one did not. The
  fixture now goes in on argv through `startup_files`, exactly like `wayland-attach-check.js`, and
  the "empty" vs "loaded" states are two separate launches instead of one launch driven by typing and
  clicking. The transport-click that produced the old "playing" state is gone entirely — 4c's ban is
  categorical for the real display, not just for typing, so there is no compliant way to click Play
  there. Losing that third state is the real, disclosed cost of compliance; the script still proves
  the thing N2b actually needed proof of, that the picture paints at all.
- **Stopped capturing the whole desktop.** `spectacle -f` plus a hand-derived `4/3` crop factor is
  replaced with `import -window <id>`, 4c's own stated method ("needs no raise and no focus"). The
  three `raise()`/`windowactivate` calls per run are gone with it — they existed only to compensate
  for the old capture method's blind spot.
- **Fixed the three minor findings alongside it**, since they lived in the same rewrite: the
  hardcoded `/home/alcahest/git/SubLore` path is now `requireAppBinary()` from `e2e/lib/paths.js`
  (same helper every other script in this directory uses); both remaining `spawnSync` calls
  (`import`, `ffmpeg`) now check `.status` explicitly instead of only the first one; and every temp
  directory the script creates (`XDG_DATA_HOME`, the screenshot dir) is `rmSync`'d in a `finally`,
  verified empty after three separate live runs (`ls /tmp | grep sublore-e2e-real` returned nothing
  each time).
- Kept the inline ffmpeg signalstats measurement rather than importing `e2e/lib/pixels.js`'s
  `saturation()`: that helper captures via `x11grab` on the X root, which this script's own header
  already documented as reading black on this rootless XWayland session — the capture step
  genuinely differs, not just the wiring, and `e2e/lib/pixels.js` is outside this cluster's file
  ownership regardless. The header now says so, so the duplication reads as a deliberate, explained
  choice instead of an unexplained copy.

**Not fixed, and why, honestly:** `package.json` and `e2e/README.md` are not in this cluster's file
list, so I did not add an npm script entry or a README row. I believe this is the right outcome, not
just a scope excuse: `n1b-load-probe.js` — the sibling the register itself treats as the sound
pattern for a self-declared probe — also has no `package.json` entry and no README row, and L9's own
report calls that fact sound rather than a finding, on the same reasoning this header now states.
If the orchestrator wants a README row anyway for discoverability, that is a one-line addition for
whoever owns that file.

Proof: run three times against the owner's real Wayland session (the only environment this probe is
for) after the rewrite, all three clean, no thrown error, no leftover process, no leftover temp
directory:

```
empty:  saturation=2.121   loaded: saturation=15.101
empty:  saturation=2.115   loaded: saturation=10.893
empty:  saturation=2.115   loaded: saturation=9.026
```

(One earlier run, before I settled the sleep timing, read `0.166`/`0.166` — a paint-timing flake on
this exact machine's own history, consistent with `video-surface.spec.js`'s documented 2-in-10 under
software rendering; the three runs above are the ones that count, all comfortably differentiated and
in the range `n2b-collaudo-reale.md` recorded.) `wayland-attach-check.js` was also re-run for real
after its own fix and passed 4/4 both times it was invoked.

## The GTK dialog-button duplication (`close-gate-check.js:124`, cross-referenced against `n1b-load-probe.js:52`)

**Fixed.** Extracted the byte-identical `clickDialogButton` into a new file,
`e2e/lib/gtk-dialog.js`, imported by both scripts. Both owned files now share one implementation
instead of two that could drift; `close-gate-check.js` keeps its three-slot map (save/discard/cancel,
cancel is actually dismissed via Escape, never a click), `n1b-load-probe.js` uses the same map and
simply never passes `"cancel"`. Proof: `pnpm e2e:close-gate` still clicks Discard and Save
correctly (12/12, including both branches' post-click `waitForDialogGone`, which would time out on a
mis-click), and `n1b-load-probe.js save` / `n1b-load-probe.js discard` both still reach `phase:
"done"` under the shared function.

## `n1b-load-probe.js:120` — the `finally`-block `SIGKILL` that could mask a near-crash

**Fixed**, as the "suspicion" it was reported as. Before, teardown unconditionally `SIGKILL`ed the
process group whether the run had reached `"done"` on its own or was still alive at the 20 s timeout
— a run cut off mid-crash and a run that finished cleanly both read the same way afterward. The
`finally` block now records `killedRunning = exit === null && processGroupMembers(pgid).length > 0`
_before_ the kill, and prints it in the JSON line. A future battery script can now tell "this run was
still alive when we cut it off" apart from "this run finished on its own and simply didn't crash" —
which is exactly the distinction the finding said was missing. Proof: `n1b-load-probe.js save` and
`... discard`, run under Xvfb, both printed `"killedRunning":false` on their normal, self-terminated
completions — the flag is wired correctly for the common case; I did not force the timeout branch
(that needs an artificially wedged app, which risks leaving a real orphan on a shared machine for no
proof beyond what reading the code already shows: `killGroup` is only reached when `killedRunning`
is computed `true` first).

## `n1b-load-probe.js:130` — the missing sixty-run, six-stream orchestrator

**Not fixed.** This script runs exactly one app instance for one branch and prints one JSON line, by
design (its own header says so). The finding is that nothing in the tree runs it sixty times across
six concurrent streams — the orchestration that produced the headline "2 in 30 on save, 0 in 30 on
discard" numbers exists only as a command someone typed once, not as committed code. Building that
orchestrator (spawn N concurrent streams, collect JSON lines, correlate `killedRunning`/`phase`
against `coredumpctl`, filter on `phase === "done"`) is a real feature addition, not a defect fix in
the sense the other rows in this cluster are: it has no existing shape to correct, and I cannot prove
a new one correct without actually running it at the scale it is meant for — sixty real app launches
in six concurrent streams, repeatedly, against the owner's own machine, purely to validate a test
harness rather than to test the product. That is a cost and a risk I am not willing to spend inside
a fix-cluster's time box on an unreviewed design. I am reporting this honestly as unfixed rather than
writing an orchestrator I could not verify.

The docs-side half of the same finding — `docs/reports/n1b-sessanta-corse.md:9` citing a probe
filename (`n1b-branch-probe.mjs`) that never existed — is in a file outside this cluster's ownership
(`docs/reports/`), so I did not touch it; noting it here so it is not silently dropped.

## Not a defect

None of the fourteen rows taken were judged sound-as-is; every one either got a code fix above or is
named as not-fixed with a reason above.

## Summary of file changes

- `e2e/scripts/close-gate-check.js` — three fake assertions replaced with real `mapState` checks;
  local `clickDialogButton` replaced by the shared one.
- `e2e/scripts/wayland-attach-check.js` — one fake assertion replaced with a real `mapState` check;
  15-line inline comment trimmed to two lines pointing at the report that already carries the detail.
- `e2e/scripts/scaled-surface-check.js` — one fake assertion replaced with a real `mapState` check,
  captured before window teardown.
- `e2e/scripts/n1b-load-probe.js` — local `clickDialogButton` replaced by the shared one;
  `killedRunning` added to the printed result and computed before teardown's `SIGKILL`.
- `e2e/scripts/real-session-check.mjs` — rewritten: declared as a probe, argv-only loading (no
  typing/clicking on the real display), `import -window` capture (no full-desktop screenshot, no
  raise/activate), `requireAppBinary()` instead of a hardcoded path, both capture subprocesses check
  their exit status, all temp directories cleaned up in `finally`.
- `e2e/lib/gtk-dialog.js` — new, shared `clickDialogButton`, used by the two scripts in this cluster
  that both needed it.

All five owned scripts plus the new shared module pass `node --check`, `prettier --check`, and
`eslint` with no findings, and were each executed for real at least twice during this session (once
before, once after the prettier reformat) with clean process and temp-file teardown confirmed by
`pgrep`/`ls` afterward.
