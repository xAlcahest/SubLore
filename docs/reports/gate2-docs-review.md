# Gate 2 — L12: documentation that outruns the code

**Question:** do the documentation commits in this range describe the tree that exists?
**Range:** `f0b0058..eca9806` (062f201, fee26f8, 3657241, 323026b, 2b31f14, 5332875, 18fe5f3, plus
the N2c delivery: c7261a5, e67bb40, eca9806).

## What I checked

- Read `3657241`, `323026b`, `5332875`, `18fe5f3` in full (`git show`), plus the doc portions of
  `062f201`, `2b31f14` and N2c (`c7261a5`, `e67bb40`, `eca9806`).
- Read every doc file touched in range in full: `BACKLOG.md`, `WORKFLOW.md`,
  `docs/design/decisions.md`, `docs/design/x11-vs-render-api.md`, `docs/reports/m2-0-prontezza.md`,
  `docs/reports/n1b-segfault-uscita.md`, `n1b-sessanta-corse.md`, `n1b-trenta-corse.md`,
  `n2b-collaudo-reale.md`, `n2b-probe.md`, `n2b-stato.md`, `n2c-p3-scala.md`.
- Resolved every path/function/line citation named in the brief against the current tree:
  `n1b-branch-probe.mjs` (assigned to L10, confirmed the plan's claim that no such file exists —
  not re-reported here), `VideoStage.tsx:26-33`, `surface/linux.rs:63-64`, `surface/mod.rs:51`,
  and `tao-0.35.3/src/platform_impl/linux/window.rs:431` plus its field at line 54 (fetched the
  vendored crate source from `~/.cargo/registry/src`).
- Compared BACKLOG's N1b and N2 acceptance criteria against the evidence tables in their own
  supporting reports.
- Read WORKFLOW §4a's gate-2 paragraph against BACKLOG.md's NOW-block ordering and
  `docs/design/m2-0-tasks.md`'s dependency statements.
- Ran the §9 sweep: `git diff $GATE_BASE $GATE_HEAD -- BACKLOG.md docs | grep -n '^+' | grep -i verified`,
  then read every hit's surrounding sentence for a missing platform qualifier, and separately grepped
  the same diff for "windows" to check for behaviour-vs-compilation conflation.
- Read the five comment sites named in the brief against CLAUDE.md §6 ("max 1–2 lines per
  guard/block... longer reasoning goes in the PR description, never inline"): `main.rs`'s
  `mitigate_nvidia_webview` docstring, `dialog.rs`'s module doc, `video-surface.spec.js`'s new
  header/inline comments, `wayland-attach-check.js:132-146`, `scaled-surface-check.js`'s header.

## Findings, most severe first

### 1. `BACKLOG.md:85` claims a visible-frame assertion that `062f201` deleted from the spec — serious

`BACKLOG.md:85` (N2's AC, `[x]`, unchanged text carried through this whole range): "the frame is
visible and playback continues. **The assertion is on the visible frame, not on an internal flag**,
because mpv creates its own window inside ours and leaves it unmapped if ours is
(`video/surface/mod.rs:82-84`) — a state-only assertion would pass while the user sees black."

`062f201` rewrote `e2e/specs/video-surface.spec.js` and removed the pixel/saturation check
entirely (deleted `PICTURE`, `saturation()`, `requireFfmpeg()`, every `waitFor(() => saturation(...) > PICTURE ...)`),
replacing it with map-state + `childWindows().length > 0` checks. The new file header says so
explicitly, at `e2e/specs/video-surface.spec.js:5-6`: "What is asserted here is the surface coming
back mapped with mpv still attached to it — **not the pixels**." The header goes on to say the
pixel check was dropped because it was unreliable under Xvfb/llvmpipe (2 of 10) and that the
picture is instead "verified on real hardware" in `docs/reports/n2b-collaudo-reale.md`.

**Fails when:** a maintainer reads BACKLOG's N2 AC, trusts that the automated Linux suite asserts
the visible frame the way the AC says it must, and does not know that the actual pixel evidence for
N2 (as opposed to N2b) is three manual launches on the owner's own hardware rather than anything CI
runs. Nothing in this range — not `062f201`, not the later N2c/N1b docs commits — touched
`BACKLOG.md:85` to say the assertion moved off pixels. The AC and the code it describes now
disagree about what "verified-by-tests" for N2 actually covers.

### 2. `BACKLOG.md:110`'s N1b AC demands 30 sequential close-gate runs; the delivered-binary evidence shows 3 — serious

`BACKLOG.md:110`: "AC: thirty sequential runs of `pnpm e2e:close-gate` stay clean, and no assertion
in it is weakened." `BACKLOG.md:106` marks N1b `[x]`, "fixed 2026-08-30."

The judgment table in `docs/reports/n1b-sessanta-corse.md:70` (added by `2b31f14`) reads:

```
| judge                                             | first binary | delivered binary |
| sequential `pnpm e2e:close-gate`                  | 30 green, 0 red | 3 green, 0 red |
```

The _first_ binary (the one written against the older, buggier hypothesis) got 30 sequential runs.
The _delivered_ binary — the one N1b's `[x]` status actually certifies — got 3. BACKLOG's own status
paragraph (`BACKLOG.md:110` area, "Judged on the delivered binary... close gate 12/12 three times")
is consistent with the report's "3 green" (three runs of the 12-assertion check, all green — not a
contradiction with the table, just two notations for the same 3 runs), so the two documents agree
with each other. What they do not agree with is the AC two lines above them, which still asks for
thirty.

**Fails when:** anyone treats N1b's `[x]` as certifying that its own written acceptance criterion
was met on the shipped binary. It was not: the AC's number and the delivered evidence's number are
30 and 3, and nothing in the range reconciled them — either by lowering the AC to match what was
actually run, or by running 30 sequential passes against the delivered binary before checking the
box.

### 3. `WORKFLOW.md:55` and `BACKLOG.md:74` disagree about what follows gate 2 — serious

`WORKFLOW.md:55` (edited by `18fe5f3`, then further edited by `e67bb40` inside this range): "2.
After N2c, **immediately before the owner's manual checklist**. Decision 1 is **not** in this gate:
owner ruling 2026-08-30 moved it into M2.0 as T3, where the plan designed it and where its
predecessors T1, T1b and T2 sit."

`BACKLOG.md:74` (edited by `5332875`, in range): "Order updated 2026-08-30 by owner ruling: **N2c
closes the NOW block, then gate 2, then M2.0 starting at T1.** Decision 1 is no longer ahead of the
gate — it is M2.0's T3."

`docs/design/m2-0-tasks.md:473-474` (untouched in range but read against the two above): "gate 2 is
its merge predecessor... The owner's 2026-08-30 ruling is N2c, then gate 2, then M2.0 starting at
T1 (`BACKLOG.md:74`)."

Two out of three sources put the whole of M2.0 (T1 through at least T3, where decision 1 lands)
between gate 2 opening and whatever comes next. The third — the sentence in WORKFLOW.md itself —
says gate 2 sits "immediately before the owner's manual checklist," with nothing named in between.
The nearest owner checklist that exists in BACKLOG is `BACKLOG.md:160`, **Owner checklist M2**,
which sits after M2.0 _and_ M2.1–M2.6. "Immediately before" that checklist is false by a whole
milestone's worth of tasks; the sentence was true before `5332875`/`18fe5f3` moved decision 1 out of
the gate and into M2.0, and the opening clause was never updated to match the change the rest of the
same sentence describes.

**Fails when:** someone reads WORKFLOW §4a's gate table top-to-bottom expecting gate 2 to hand off
directly to a manual checklist, and either skips scheduling M2.0's T1–T9 work or is surprised that
"immediately before" meant "with a milestone in between."

### 4. Three documents cite `physical()`/`logical()`, functions the range's own fix renamed away — serious

`c7261a5` ("fix: resolve the video region to native pixels in the page") deleted
`SurfaceRegion::logical()` and `SurfaceRegion::physical()` from `src-tauri/src/video/surface/mod.rs`
and replaced them with `pixels()` and `pixels_over(divisor)` (diff confirmed: the old
`fn logical(&self)` / `fn physical(&self)` block is removed, the new `pub fn pixels` /
`pub fn pixels_over` block is added). `surface/mod.rs:51` today is `pub fn pixels_over`, not
`physical()`; there is no `fn physical` or `fn logical` anywhere under `src-tauri/src/video/surface/`
any more (checked with `grep -rn`).

Three documents in this same range still cite the old names as if they were current:

- `docs/reports/n2c-p3-scala.md:26,44,46,48` — "`physical()` in `surface/mod.rs:51` multiplies by a
  factor... `logical()` — the path Linux actually uses (`surface/linux.rs:63-64`)... both `logical()`
  and `physical()` are wrong here... `physical()` multiplies by it today." This file was itself
  amended by `c7261a5` (it gained the "Verified on the owner's display" closing section in the same
  commit that renamed the functions it's describing), so the rename and the stale citation landed in
  the same commit.
- `BACKLOG.md:99` (added by `18fe5f3`, N2c entry): "so `physical()` would multiply by 1 and
  `logical()` by nothing... since `scale_factor()` there does carry the ratio and `physical()`
  already multiplies by it."
- `docs/design/x11-vs-render-api.md:35,153` (added by `323026b`, before the rename): "`set_region`
  calls `region.logical()`, which is `scaled(1.0)`..." and "...that `logical()` is scale-independent,
  that `physical()` scales..." `scaled()` is also gone — it was the private helper both old methods
  called, removed in the same `c7261a5` diff.

**Fails when:** a reader of `x11-vs-render-api.md` — the document `decisions.md`'s new §14 explicitly
defers to for "the reasoning, the costs and the three conditions" behind keeping the X11 child window
for v1.0 — goes looking for `logical()`/`physical()`/`scaled()` in `surface/mod.rs` or
`surface/linux.rs` to check the claim and finds none of the three. The document underlies a live
architecture ruling and none of the three sites was corrected after the rename that made them stale,
even though `c7261a5` is the last code commit in the range and directly touched two of the three
files carrying the citation.

### 5. `src-tauri/src/main.rs:4-12` — measurement-heavy docstring, not a 1–2 line guard comment — minor

The doc comment on `mitigate_nvidia_webview` runs 9 content lines and carries specific measured
numbers that read like a PR/report excerpt pasted inline: "Measured on an RTX 5070 Ti with driver
610.57.04... the window is flat at 46..46... Only with the DMABUF renderer off does the interface
appear, at 16..235." CLAUDE.md §6: "Comments: max 1–2 lines per guard/block, reference the issue
number; longer reasoning goes in the PR description, never inline." This is attached to one
function implementing one narrow workaround (closer to a "block" than a module), and the
measurement detail duplicates what belongs in the commit/PR body or a report — nothing here is a
citation error, just length and content that the project's own comment policy caps.

Distinguishing this from a legitimate exception: `dialog.rs`'s file-level `//!` doc (11 lines,
explaining why the whole file exists architecturally) and the file headers on
`video-surface.spec.js` and `scaled-surface-check.js` are module/file-level documentation, not
guard-clause narration, and are consistent with the rest of the codebase's convention for test-file
headers — I did not flag those (see "sound" below). The `main.rs` docstring is different: it sits on
a single function and carries raw measurement data rather than architectural rationale.

### 6. `e2e/scripts/wayland-attach-check.js:132-146` — 15-line inline block narrating an experiment — minor

Mid-function (inside the check that asserts mpv's attachment, not at the file header), a 15-line
comment explains why the picture assertion was dropped, restating rates and numbers already on
record in `docs/reports/n2b-collaudo-reale.md`: "showed 2 times in 10 while mpv was attached in all
10... three runs out of three showed the frame, saturation 5.86 against 2.1 for the empty shell... "
This is exactly the shape CLAUDE.md §6 and the brief's own example call out — "a six-line block
inside a `before()` hook probably is not [legitimate]" — a comment inline at the point of one
specific assertion decision, not a file/module header. The reasoning and the numbers are already
written down in the linked report; the inline copy should be 1-2 lines pointing there.

## Hunt items checked and found sound

- **`n1b-branch-probe.mjs` citation** (`docs/reports/n1b-sessanta-corse.md:9`): confirmed no file by
  that name exists in the repo's history; the committed script is `e2e/scripts/n1b-load-probe.js`
  (added in `3657241`). The brief assigns this one to L10 explicitly ("see L10"); I verified it
  resolves the way the plan already states and did not re-report it as my own finding.
- **`VideoStage.tsx:26-33`**: the "resolved here, in native pixels" comment and the
  `getBoundingClientRect()`/`devicePixelRatio` lines it describes are exactly at that range. Matches
  every citation of it in `n2c-p3-scala.md` and `BACKLOG.md`.
- **`surface/linux.rs:63-64`**: `set_region`'s comment ("GDK multiplies child geometry by this
  factor on the way to X...") and the `pixels_over(...)` call sit at exactly those two lines.
- **`surface/mod.rs:51`**: `pub fn pixels_over(&self, divisor: f64)` is at line 51, matching every
  citation that treats line 51 as the region-conversion function (though see finding 4 — several
  callers of that citation still name it `physical()` instead of `pixels_over`).
- **`tao-0.35.3/src/platform_impl/linux/window.rs:431`**: fetched the vendored crate from
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tao-0.35.3/`. Line 431 is
  `self.scale_factor.load(Ordering::Acquire) as f64` inside `pub fn scale_factor(&self) -> f64`, and
  the field at line 54 is `scale_factor: Rc<AtomicI32>`, exactly as `n2c-p3-scala.md:22` describes.
- **§9 bare-"verified" sweep** (`git diff ... | grep -i verified` over every added line in
  `BACKLOG.md`/`docs`): every behavioural verification claim added in this range carries an explicit
  platform ("on Linux", "on the owner's own Wayland session", "on the owner's 1.5 display", "on the
  real session"). The handful of non-platformed "verified" hits are all about static facts (source
  code, crate manifests, string existence) rather than runtime behaviour, e.g. "verified in the
  vendored source" (a `RenderParam` enum has no SW variant) and "verified in the crate's manifest" —
  these are not behavioural claims and don't need a platform. No bare, unplatformed behavioural
  "verified" found.
- **Windows behaviour-vs-compilation sweep**: every Windows-related line added in this range either
  explicitly disclaims behaviour ("Windows compiles in CI and has never had these checks run against
  it," "a green compile is not a behavioural result," "the Windows change is labelled
  compiled-not-run," "the Windows branch... has never been executed anywhere") or states a code fact
  (function names, `cfg` gates) rather than a runtime claim. No instance found where Windows
  compilation is presented as Windows behaviour.
- **`dialog.rs`'s module doc** (`//!` block at the top of the file, 11 lines): file-level
  architectural rationale for why the file exists and what it deliberately does not fix — matches
  the brief's own example of a likely-legitimate module doc. Not flagged.
- **`video-surface.spec.js`'s new header** (lines 1-16) and **`scaled-surface-check.js`'s header**
  (lines 1-16): both are file-level JSDoc explaining test scope and known limitations, consistent
  with every other check file in `e2e/scripts/` and `e2e/specs/` (all of which carry similarly-sized
  headers as an established codebase convention, not something introduced in this range). Not
  flagged, unlike the mid-function block in finding 6.
- **`docs/design/decisions.md`'s new §14**: reads consistently with `x11-vs-render-api.md`, the
  document it defers to; no internal contradiction found beyond the stale function names already
  covered in finding 4. (A duplicated `---` separator line is a formatting slip, not a finding —
  prose/formatting is explicitly out of scope for this lens.)
