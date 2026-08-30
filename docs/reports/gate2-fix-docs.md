# Gate 2 — Wave 3, documentation cluster (BACKLOG.md, WORKFLOW.md, x11-vs-render-api.md, decisions.md, n1b-sessanta-corse.md)

Scope: every register row whose site sits in `BACKLOG.md`, `WORKFLOW.md`, `docs/design/x11-vs-render-api.md`,
`docs/design/decisions.md`, `docs/reports/n1b-sessanta-corse.md`. Eight rows: five serious, three minor.
Read against `docs/reports/gate2-docs-review.md` (L12), `docs/reports/gate2-n1b-evidence-review.md` (L10) and
`docs/reports/gate2-n2c-region-review.md` (L11) before touching anything, per the brief.

I did not run `cargo test`, `cargo clippy`, or rebuild the app: nothing outside the five files above was
touched, and no behaviour changed. Proof for a documentation fix is that the corrected text now agrees with
the code, the commit history, or an explicit statement that the underlying data does not exist — each row
below names which kind of proof it got.

## Serious

### 1. `BACKLOG.md:85` — N2's AC claims a visible-frame assertion `062f201` removed from the spec — fixed

The AC said the automated suite asserts "the frame is visible," with the reasoning that a state-only
assertion would pass while the user sees black. `062f201` rewrote `video-surface.spec.js` and replaced the
pixel/saturation check with map-state + `childWindows().length > 0` checks — the file's own header already
says so — and that removal was never named in `062f201`'s delivery description, which WORKFLOW §4 requires
for a weakened assertion.

**Fix:** added a `Correction, gate 2` note under the AC stating plainly that the automated suite does not
assert the visible frame, that the removal went unnamed, and that the pixel half of the claim rests on three
manual launches (`docs/reports/n2b-collaudo-reale.md`), not CI. Kept the original reasoning, since it is
still why the suite checks mpv's mapped child window rather than an internal flag. Also fixed the AC's own
citation, `video/surface/mod.rs:82-84`, stale since `c7261a5` added 146 lines to that file — the comment it
points at is now at `:98-99`.

**Proof:** the corrected line names the exact commit, the exact file header sentence, and the exact citation
range, each verified against the current tree (`git show 062f201 -- e2e/specs/video-surface.spec.js`,
`sed -n 96,102p src-tauri/src/video/surface/mod.rs`). A reader who follows any of the three citations now
finds what the text says they will find.

**State: fixed.**

### 2. `BACKLOG.md:110` — N1b's AC demands thirty sequential close-gate runs; delivered-binary evidence shows three — fixed as a correction, not as a re-run

The AC still reads "thirty sequential runs of `pnpm e2e:close-gate` stay clean." The evidence for the
delivered binary, in the same document (`n1b-sessanta-corse.md:70`) and in BACKLOG's own status line two
lines below the AC, is three. Only the pre-fix binary got thirty.

I did not attempt to close this by running the missing twenty-seven — that would be new verification work,
not a documentation fix, and this task's scope is the five files, not re-running the app's close path
twenty-seven more times to manufacture a number the record never claimed to have. The honest fix, per
CLAUDE.md §5.4 ("if a test is wrong, say so explicitly and fix it as its own change") and the plan's own
instruction ("lowering the AC to match what was actually run, or running thirty passes before checking the
box"), is the first of those two, done explicitly and named.

**Fix:** added a `Correction, gate 2` note directly under the AC, stating the delivered binary was run three
times, not thirty, quoting the exact table row that proves it, and naming that the gap was never closed
either way. The AC's original text is left standing, not deleted, so the record shows what was claimed and
what is true side by side.

**Proof:** the quoted table row (`n1b-sessanta-corse.md:70`) is unchanged and independently readable; the
correction does not assert a number, it names the discrepancy between two numbers already in the tree.

**State: fixed** as a documentation correction. **Not fixed** as a re-verified AC — the underlying gap (only
three of the required thirty runs exist against the delivered binary) is a real open item, now visible
instead of hidden by a checkbox. If the owner wants the AC actually met, that is thirty more runs of
`pnpm e2e:close-gate`, a task outside this cluster's file ownership and outside "documents that outran the
code."

### 3. `WORKFLOW.md:55` — gate-2 line contradicts BACKLOG's own ordering — fixed

`WORKFLOW.md:55` said gate 2 sits "immediately before the owner's manual checklist." `BACKLOG.md:74` and
`docs/design/m2-0-tasks.md:473-474` both say the order is N2c, then gate 2, then M2.0 starting at T1 — a
whole milestone (M2.0 through M2.6) between the gate and the nearest owner checklist, `BACKLOG.md`'s
**Owner checklist M2**. The opening clause was true before the owner moved decision 1 out of the gate and
into M2.0's T3, and was never updated when the rest of the sentence was.

**Fix:** changed "immediately before the owner's manual checklist" to "immediately before M2.0 starts,"
and added a sentence naming what actually follows: M2.0 through M2.6 sit between the gate and Owner checklist
M2, with a citation to both (`BACKLOG.md:74`, `BACKLOG.md:163`).

**Proof:** both citations point at the current tree — `BACKLOG.md:74` still carries the "N2c closes the NOW
block, then gate 2, then M2.0 starting at T1" sentence, and `BACKLOG.md:163` is the `Owner checklist M2`
line, confirmed with `grep -n` after the BACKLOG edits above landed (line numbers shifted by my own BACKLOG
edits, and I re-checked the citation against the post-edit file rather than the pre-edit one).

**State: fixed.**

### 4. `docs/design/x11-vs-render-api.md:153` (and `:35`) — three documents cite `physical()`/`logical()`/`scaled()`, functions `c7261a5` renamed away — fixed in the two documents I own

`c7261a5` deleted `SurfaceRegion::logical()`, `::physical()` and the shared `::scaled()` helper from
`surface/mod.rs`, replacing them with `pixels()` and `pixels_over(divisor)`. The L12 finding names three
stale citations: `docs/reports/n2c-p3-scala.md` (not my file — left untouched, out of scope), `BACKLOG.md:99-100`,
and `docs/design/x11-vs-render-api.md:35,153`.

**Fix, `x11-vs-render-api.md`:** rewrote the N2c mechanism paragraph (`:35`) into explicit past tense,
labelled "diagnosis history, not the current code," and added the rename (function names, replacement names,
new line numbers) at the end of the paragraph. Rewrote the `:153` action-item bullet ("N2c needs P3 before it
needs code") to state the action was taken and the shipped unit tests are what it asked for, rather than
leaving an open task pointed at a mechanism and an API that no longer exist.

**Fix, `BACKLOG.md:99-100`:** same treatment — past tense, explicit "at the time," and an appended
`Correction, gate 2` note naming the rename and the replacement functions.

**Not touched:** `docs/reports/n2c-p3-scala.md` — not in this cluster's file list, belongs to another
implementer or is out of scope for gate 2's file-ownership split. Named here so it is not silently dropped.

**Proof:** `surface/mod.rs:43` is `pub fn pixels`, `:51` is `pub fn pixels_over(&self, divisor: f64)`,
confirmed by direct read of the current file; `grep -n "logical()\|physical()\|scaled("` over both edited
documents now shows every remaining hit inside past-tense, explicitly-labelled historical prose, none
presented as the current API.

**State: fixed**, in the two files this cluster owns.

### 5. `docs/reports/n1b-sessanta-corse.md:33` — the six-stream table has no "reached the end" column — corrected, not fabricated

The sequential battery table has `runs | reached the end | non-zero exit | signal | core dump`. The
six-stream table — the one that produced "2 in 30 on save, 0 in 30 on discard" and everything built on that
split — has only `runs | SIGSEGV | core dump`. A run that timed out under X11 focus contention before
reaching `"done"` would print zero SIGSEGV and zero core dump, indistinguishable in this table from a clean
completed run.

I could not add real "reached the end" numbers to this table: the code that ran the probe sixty times across
six concurrent streams and collected the results was never committed (confirmed: no script and no
`package.json` entry does this; `git log --all --diff-filter=A --name-only` finds nothing that would),
so the raw per-run data this table was built from does not exist anywhere in the tree. Inventing numbers to
fill the gap would be exactly the "fake a pass" CLAUDE.md §5.4 forbids, just aimed at a report instead of a
test.

**Fix:** added a `Correction, gate 2` paragraph immediately under the table, naming the missing column, the
mechanism that makes it ambiguous (`n1b-load-probe.js:118-119`'s silent catch plus `phase` being the only
signal), and stating explicitly that the apparatus that produced these numbers is not in the tree to
re-check. Named that the later judgment table (added by `2b31f14`, further down the same file) does carry a
"done" figure and is not affected by this gap, so the correction doesn't cast doubt on evidence that already
answers it.

**Proof:** the correction is verifiable by reading `n1b-load-probe.js:118-119` (silent catch confirmed) and
by the absence of any aggregating script in `git log --all --diff-filter=A`. There is no way to "prove" a
missing measurement was taken; the proof here is that the gap is now stated rather than hidden.

**State: fixed** as a documentation correction — the record now says what the table can and cannot support.
**Not fixed** as an evidence gap: whether "load is a condition of the defect, save is not special" survives
scrutiny of runs that might have timed out rather than completed is still genuinely unknown, and stays
unknown until someone re-runs the battery with the column added to the probe. That's a code change to
`e2e/scripts/n1b-load-probe.js`, outside this cluster's file ownership.

## Minor

### 6. `BACKLOG.md:104` — two evidence claims the durable record doesn't carry — fixed

(a) The status line stated "Windows takes the numbers as they are" as settled fact with no platform mark,
though the commit message itself says Windows has never been run. (b) AC1's screenshot evidence
(`n2c-p3-scala.md:73`, "a capture of the window shows the picture inside the stage") names no capture
command and no image is committed.

**Fix:** appended "Windows compiles in CI; the Windows path above has never been run" to the status line.
Added a `Correction, gate 2` note under it naming the unrepeatable screenshot sentence and stating that only
the measured half of AC1 (the geometry numbers) is independently re-checkable.

**Proof:** `git ls-files` confirms no committed image; the geometry numbers (592x180 at 432,500) are
re-derivable from the page's own formula, `round(288×1.5)=432` etc., which I recomputed by hand against the
stage rectangle already on record — matches to the pixel.

**State: fixed.**

### 7. `docs/design/decisions.md:21` — citation drift into `surface/mod.rs`, caused in range and not fixed in range — fixed

`c7261a5` added 146 lines to `surface/mod.rs`. `decisions.md:21` cited `surface/mod.rs:80` for "raises above
the webview" (now `impl VideoSurface {`, the real doc comment is at `:93`) and `linux.rs:66` for `raise()`
(now `move_resize`, `raise()` moved to `:67`). `decisions.md:29` cited `surface/mod.rs:82-84` for the
show()-must-precede-mpv comment (now at `:98-99`).

**Fix:** updated both citations to the current line numbers, and noted in-line that the shift was caused by
`c7261a5`'s addition, so a future reader isn't left wondering why the numbers moved.

**Not touched:** `decisions.md:29`'s other citation, `video/mod.rs:106`, which BACKLOG.md's own M2 preamble
and `x11-vs-render-api.md`'s closing paragraph already flag as separate, pre-existing debt (stale since
`d224f3c`, before this gate's range) — not caused by this range and not part of this register row.

**Proof:** `surface/mod.rs:93` is `set_region`'s doc comment, `:98-99` is `show`'s, `linux.rs:66-67` is
`move_resize` then `raise()` — all confirmed by direct read of the current files.

**State: fixed.**

### 8. `docs/design/x11-vs-render-api.md:25` — the adopted decision-14 document still describes the pre-N2c contract — fixed

The document, which `decisions.md` §14 adopts and BACKLOG.md makes required pre-M2.0 reading, said
`VideoStage` measures the stage "with `getBoundingClientRect` in CSS pixels" and that the value flowing to
`apply_region`/`set_region` is in CSS pixels. Since N2c's fix, the page multiplies by `devicePixelRatio`
before the value ever crosses the IPC boundary — what reaches `apply_region` is already native device
pixels. (The `:35` and `:153` halves of this same finding are fixed under item 4 above, since both lenses
reached overlapping text in the same document.)

**Fix:** rewrote the sentence to say `getBoundingClientRect` reports CSS pixels but the report that actually
crosses the boundary is already multiplied to native pixels, with a citation to `VideoStage.tsx:26-41` and a
note that this is N2c's fix correcting what the document said before it.

**Proof:** read `VideoStage.tsx:22-41` directly — `rect = element.getBoundingClientRect()` at `:30`,
`ratio = window.devicePixelRatio` at `:31`, and every coordinate in the `onRegionChange` call at `:36-41` is
`Math.round(... * ratio)`. The corrected sentence matches the code line for line.

**State: fixed.**

## Summary

All 8 rows addressed: 6 fixed outright (the text now matches the code or the commit history), 2 fixed as
honest corrections while leaving a real, named gap open underneath (row 2's AC still isn't met by the
delivered-binary evidence; row 5's missing "reached the end" data still cannot be reconstructed from what's
in the tree). Nothing was deleted to make a contradiction disappear — every correction is additive, quoting
what was claimed next to what is true, per WORKFLOW's own rule for this kind of fix. No file outside
`BACKLOG.md`, `WORKFLOW.md`, `docs/design/x11-vs-render-api.md`, `docs/design/decisions.md`,
`docs/reports/n1b-sessanta-corse.md` was touched.

One item noticed in passing and explicitly not acted on, named rather than silently fixed: `decisions.md:29`
still cites `video/mod.rs:106` for `show()`'s call site, a citation already known stale (since `d224f3c`) and
already tracked in BACKLOG's M2 preamble and in `x11-vs-render-api.md`'s own closing paragraph as pre-M2.0
work. It is not caused by this gate's range and not a row assigned to this cluster, so it stays as it was.
