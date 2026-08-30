# Gate 2 — L3: what the suite actually proves

Scope: `f0b0058..eca9806`. Question: does every check in the touched tests fail for a cause the
test itself constructs; were deleted assertions replaced by something equally strong; which fix in
this range has no check CI runs. Brief: `docs/reviews/gate-2-plan.md` §2, L3 (absorbs the cut
"orphaned harness code" lens).

## What was checked

- `git diff f0b0058 eca9806 -- e2e/specs/video-surface.spec.js` in full, and the file as it stands.
- `e2e/scripts/wayland-attach-check.js`, `e2e/scripts/scaled-surface-check.js`,
  `e2e/scripts/close-gate-check.js`, `e2e/scripts/n1b-load-probe.js` — every `check()` call in each,
  traced back to whether the value it tests is already guaranteed by a preceding `waitFor`.
- `e2e/lib/proc.js` (`waitFor`'s actual contract: returns a truthy value or throws, never resolves
  falsy) and `e2e/lib/x11.js` (`childWindows`, `mapState`, what `xwininfo -tree` actually reports for
  an unmapped-but-existing child).
- `e2e/lib/pixels.js`, `e2e/wdio.conf.js`, every spec file, `e2e/scripts/real-session-check.mjs`, for
  who still calls `saturation()` and `requireFfmpeg()`.
- `package.json`, `.github/workflows/ci.yml`, for which `pnpm e2e:*` scripts run on every push.
- `BACKLOG.md`'s N2 and N2b entries against the diff of `video-surface.spec.js`, for whether the
  pixel-assertion removal was named in the delivery description per WORKFLOW §4.
- `e2e/scripts/n1b-load-probe.js` in full, and every `package.json`/`ci.yml` reference to it (none).
- Duplication of the GTK dialog-button geometry constant between `close-gate-check.js` and
  `n1b-load-probe.js`.
- `src-tauri/src/video/surface/mod.rs`'s seven unit tests, for whether `pixels()`/`pixels_over()`
  coverage exists (it does; not a finding — noted under "sound" below).

## Findings, most severe first

### 1. Three of `close-gate-check.js`'s twelve counted checks can never fail — `e2e/scripts/close-gate-check.js:261`, `:283`, `:322`

**Severity: serious.**

`EXPECTED_CHECKS = 12` (line 39) is meant to guarantee twelve assertions that can each fail for a
cause the test constructs (WORKFLOW §2.5b, "no assertion on a constant"). Three of the twelve
cannot:

```js
const dialog = await waitForDialog(state);
check("the close request raised the dialog instead of closing", dialog !== null);   // :261
...
const again = await waitForDialog(state);
check("a second close request raises the dialog again", again !== null);            // :283
...
const dialog = await waitForDialog(state);
check("the save branch reached the dialog", dialog !== null);                       // :322
```

`waitForDialog` (lines 79-97) is built on `waitFor`, and `waitFor` (`e2e/lib/proc.js:13-30`) either
returns the probe's truthy value or throws after the deadline — it never resolves with a falsy
value. `waitForDialog`'s own `.catch` (line 94-96) re-wraps any timeout into a thrown `Error`
rather than swallowing it to `null`. So by the time execution reaches `dialog !== null`, `dialog` is
already guaranteed non-null by the `await` on the line above: if the dialog had never appeared, the
script would already have thrown out of `waitForDialog` and these lines would never run at all.

**Failure this can't catch:** any regression that makes the close-gate dialog take longer than 15 s
to appear, or a code path that raises a _different_ window with the same behaviour, is caught by
`waitForDialog`'s own throw — correctly failing the run, but _not_ through these three `check()`
calls, which just restate a fact `waitFor` already established. Removing all three lines would not
change what this script can detect; it would only drop the counter from 12 to 9. That means
`EXPECTED_CHECKS = 12` currently counts three checks that cannot fail, so the guard is inflated by
25%: a future edit that deletes one of these three (say, accidentally, while refactoring the dialog
wait) and drops the counter to 11 in the same edit would pass the `checksRun < EXPECTED_CHECKS`
guard's _intent_ (nothing meaningful was lost) while the guard's actual job — catching a removed
assertion — never distinguishes a dead check from a live one.

This is the identical pattern found in `wayland-attach-check.js:112` (below) and
`scaled-surface-check.js:148` (below): three separate files inherited the same shape, most likely
because `close-gate-check.js`'s existing `dialog !== null` idiom was the template the two new
scripts copied. WORKFLOW §5b names this defect class explicitly ("no assertion on a constant... a
check whose condition the test itself guarantees") and says three prior reviews already found it —
this gate finds it again, now in three places at once.

### 2. `wayland-attach-check.js:112` — the first of its four counted checks can never fail

**Severity: serious.**

```js
const toplevel = await waitFor(
  () => { if (exit !== null) { throw ... } return findToplevel(); },
  { timeout: 30000, message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel` },
);
check("the app window appeared", toplevel !== null);
```

Same shape as finding 1: `waitFor` cannot return `toplevel` as anything but truthy. `EXPECTED_CHECKS
= 4` (line 41) therefore counts one dead check; only three of the four can genuinely fail (verified:
`mapState(surface.id) === "IsViewable"` at line ~117 and `attached !== null` at line ~125-130 are
each testing a condition distinct from what the preceding `waitFor` established — a window can be a
child in the X tree without being mapped, and `attached` genuinely can be `null` because that
specific `waitFor` is wrapped in `.catch(() => null)` rather than left to throw, so those two are
real).

### 3. `scaled-surface-check.js:148` — the first of its five counted checks can never fail

**Severity: serious.**

```js
const single = await measureAt(1);
const double = await measureAt(2);
...
check("the app came up at both ratios", single.surface !== null && double.surface !== null);
```

`measureAt` (lines 71-113) returns `{ toplevel, surface, exit, survivors }` where `surface` comes
from `const surface = await waitFor(() => surfaceOf(toplevel), {...})` (lines 88-91) — again,
`waitFor`'s contract makes a falsy `surface` impossible on a normal return. `EXPECTED_CHECKS = 5`
counts one dead check; the other four (toplevel doubled, surface doubled in size, surface doubled in
position, both runs exited 0 with no survivors) are each a real, independently-failable condition —
confirmed by reading `doubles()` (lines 122-130) and the geometry each check compares.

**Combined weight of findings 1-3:** across the three scripts touched or added in this range, 5 of
21 counted `EXPECTED_CHECKS` (24%) cannot fail. The self-counting guard these scripts were built
around (`e2e/README.md`: "Gutting an assertion has to be as red as failing one, so the checks count
themselves") is real and does its job for the other 16 — but its accuracy depends on every counted
check being live, and a quarter of them are not.

### 4. The pixel-assertion removal from `video-surface.spec.js` was never named in the delivery it shipped in — `BACKLOG.md:85` vs `062f201`

**Severity: serious.**

WORKFLOW §4: "Any test weakened, skipped, or deleted must be named in the delivery description with
the reason." WORKFLOW §2 defines "delivery description" as the BACKLOG.md status text written when a
task is marked done (item 7-8 of the task loop).

`062f201` removed the `saturation(...) > PICTURE` pixel assertion from all four places it appeared
in `e2e/specs/video-surface.spec.js` (before-hook precondition, hide check, show check, ten-cycle
check — confirmed via `git diff f0b0058 eca9806 -- e2e/specs/video-surface.spec.js`) and replaced it
with `mapState(...) === "IsViewable" && childWindows(surface.id).length > 0`. That file's own
docstring explains the change at length and cites a measurement (2/10 under Xvfb+llvmpipe, 10/10 for
mpv attachment) — this is not undisclosed anywhere, and per this brief's own most-likely-false-
positive warning, that measurement is real and does support relocating the pixel proof to real
hardware.

What is missing is the specific thing WORKFLOW §4 asks for: the delivery description. `062f201`'s
commit message is the one-line `fix: attach mpv and paint the webview in a real Wayland session`,
and the BACKLOG.md entry this commit updates and marks `[x]` is **N2b** (`BACKLOG.md:89-96`, whose
own status text at line 93 describes only the Wayland-attach mechanism and the two defects filed —
N2c and N1b — never the change to `video-surface.spec.js`). **N2**, the task whose AC the removed
assertion actually belonged to, sits three lines above at `BACKLOG.md:84-85` and was not touched by
this commit at all — it is still marked `[x]` with its original AC unedited: "The assertion is on
the visible frame, not on an internal flag... a state-only assertion would pass while the user sees
black." That sentence describes exactly what `062f201` did — replaced a pixel assertion with a
state-plus-child-window assertion — and calls it out as the failure mode the AC exists to prevent,
in the one document WORKFLOW says has to carry the disclosure.

**Judging the substitution on its merits** (per this brief's own instruction, not just because an
assertion disappeared): `mapState(...) === "IsViewable"` is the exact flag N2's AC names as
insufficient by itself. `childWindows(surface.id).length > 0` proves mpv attached its own subordinate
window, which is new information the AC didn't originally ask for and is a real, non-trivial signal
— but it still is not a pixel measurement, and the docstring says so itself ("What is asserted here
is the surface coming back mapped with mpv still attached to it — not the pixels"). The delivered
check is weaker than the AC as written, for a reasoned cause, verified elsewhere (real hardware) —
but nothing in BACKLOG.md says the AC was thereby amended, and the task the change actually lives
under (N2b) doesn't mention the change at all. A reader of BACKLOG.md alone, without opening the spec
file's docstring or `docs/reports/n2b-collaudo-reale.md`, would believe N2's pixel assertion still
stands.

### 5. `requireFfmpeg()` is a prerequisite of the whole WebDriver suite for a capability nothing in it uses any more — `e2e/wdio.conf.js:34`

**Severity: minor.**

`requireFfmpeg()` runs unconditionally before any spec starts (`e2e/wdio.conf.js:34`, file itself
untouched in this range — confirmed via `git diff f0b0058 eca9806 -- e2e/wdio.conf.js`, empty). Its
sole purpose, per its own docstring in `e2e/lib/pixels.js:4-16`, is measuring whether the video
surface shows a picture. Before `062f201`, `video-surface.spec.js` was that measurement's only
caller in the WebDriver suite. After `062f201` removed that caller (finding 4), grepping every spec
under `e2e/specs/` for `saturation` or `requireFfmpeg` returns nothing: **no spec run by `pnpm e2e`
uses ffmpeg for anything.** `saturation()` itself (`e2e/lib/pixels.js:36`) is now imported nowhere in
the repository except inside `pixels.js`'s own module — confirmed with
`grep -rn pixels.js e2e/`, which returns only `wdio.conf.js`'s `requireFfmpeg` import.
`real-session-check.mjs:68-95` reimplements the same ffmpeg-signalstats measurement inline rather
than calling the shared function, and `wayland-attach-check.js` only mentions "saturation" in a
comment.

**Failure this causes:** a CI runner or a developer machine missing ffmpeg fails the entire
WebDriver suite at startup (`requireFfmpeg`'s own error) even though, as the tree stands, not one of
the 33 tests that suite runs would have used it. This is exactly "a prerequisite enforced for
nothing." It is not dead code in the strict sense — `saturation()` still does real work when called
directly by hand or by a script outside the WebDriver run — but the _automatic, blocking_ check in
`wdio.conf.js` no longer guards anything the automated suite does. The right fix is very likely
consolidation (point `real-session-check.mjs` at the shared `saturation()` instead of its own copy,
then decide whether `requireFfmpeg()` still belongs in `wdio.conf.js` at all) rather than deletion —
the N2b reports depend on the measure, as the brief anticipates.

### 6. `pnpm e2e:scale` — N2c's own regression guard — runs in no CI job, and nothing says why — `package.json:19`, `.github/workflows/ci.yml`

**Severity: serious.**

`.github/workflows/ci.yml`'s `e2e` job runs exactly `pnpm e2e`, `pnpm e2e:shutdown`,
`pnpm e2e:close-gate` (confirmed: full file read, no other `pnpm e2e:*` invocation exists anywhere
in the workflow). `pnpm e2e:scale` (`e2e/scripts/scaled-surface-check.js`) and `pnpm e2e:wayland`
(`e2e/scripts/wayland-attach-check.js`) are both absent.

The two are not the same case. `wayland-attach-check.js` states its own CI exclusion explicitly and
gives the reason: "Needs a real Wayland socket, so it runs on a machine with a Wayland session and
is not part of the headless Linux CI job" (its own docstring, lines 17-18) — this brief's own rule
is that a check legitimately unable to run in CI is not itself a finding once it says so, and this
one does.

`scaled-surface-check.js` needs no such exclusion and does not claim one. It runs under plain X11
with `GDK_SCALE` set (confirmed: `requireDisplay()`, no Wayland dependency anywhere in the file), the
exact kind of display Xvfb already provides for every other job in this workflow — the script's own
header explains it tests _integer_ GDK scaling specifically because that case, unlike N2c's
fractional case, _can_ be produced under Xvfb. Nothing in the script, in `BACKLOG.md`'s N2c entry
(`BACKLOG.md:104`, which discusses only what the check _cannot_ prove about fractional scaling, never
whether it runs automatically at all), or in `e2e/README.md` (which does not mention
`scaled-surface-check.js` at all — it postdates the README's spec table) states that this check is
excluded from CI or why.

**What this leaves unguarded:** `scaled-surface-check.js` is the one automated check that would
catch the exact regression its own header names as the reason it exists — "the surface land[ing] at
four times its rectangle instead of two" if `surface/linux.rs`'s divide-by-scale-factor logic is
ever broken by a later change. The Rust unit tests in `surface/mod.rs` cover the arithmetic in
isolation; this script is the only one that drives the real GDK-to-X11 pipeline end to end. As it
stands, a regression there ships to every `main` push undetected by CI, and neither the code nor the
docs say so out loud — which is exactly the class this brief asks about ("a fix with no check that
CI runs and no statement that it has none").

### 7. `close-gate-check.js` and `n1b-load-probe.js` each carry their own copy of the GTK dialog-button geometry — `e2e/scripts/close-gate-check.js:124`, `e2e/scripts/n1b-load-probe.js:52`

**Severity: minor.**

```js
// close-gate-check.js:117-129
function clickDialogButton(dialog, which) {
  const slots = { save: 2, discard: 1, cancel: 0 };
  const slot = slots[which];
  const buttonWidth = 96;
  const x = dialog.absX + dialog.width - 24 - buttonWidth / 2 - slot * (buttonWidth + 12);
  ...
}
```

```js
// n1b-load-probe.js:45-57
function clickDialogButton(dialog, which) {
  const slot = { save: 2, discard: 1 }[which];
  const buttonWidth = 96;
  const x = dialog.absX + dialog.width - 24 - buttonWidth / 2 - slot * (buttonWidth + 12);
  ...
}
```

Byte-identical formula, independently written, in two files both added or edited in this range
(`n1b-load-probe.js` is new in `3657241`; `close-gate-check.js`'s copy predates the range but was
touched by it — see finding 1). CLAUDE.md §6: "reuse what exists." If the real dialog's button width
or gap ever changes (a GTK theme change, a font-size change, a label change — this brief's L4
counterpart already flags this number as author-machine-specific), both copies have to be updated
together and nothing enforces that; a fix to one that misses the other produces a script that
mis-clicks the dialog and, per this brief's "does not count" note on `n1b-sessanta-corse.md`, could
misattribute a click on Discard to a "successful save" run without either script's own assertions
catching it (that specific consequence is L10's territory; this finding is just the duplication that
makes it possible).

## Hunt items checked and found sound

- **`video-surface.spec.js`'s `before()` hook, the specific replacement of `waitFor` with an instant
  `if` check (`:153-155`).** This is _not_ a dead check in the same sense as findings 1-3: unlike
  those, `if (childWindows(surface.id).length === 0) throw` immediately follows a `waitFor` at
  `:140-152` whose own probe already required `childWindows(found.id).length > 0` to resolve. So this
  one _is_ the same defect class — a condition the immediately preceding line already established —
  but I am not counting it separately from findings 1-3 above; it is the fourth instance of the exact
  same pattern, in the fourth file, and I fold it into the same defect class rather than listing it a
  sixth+ time. Net: the pattern in findings 1-3 recurs here too. (Counted findings 1-3 for
  concreteness of file:line; this is the same shape and the orchestrator should treat all four sites
  as one defect class per the Wave 2 dedup rule.)
- **`n1b-load-probe.js`'s unconditional `process.exit(0)` and silent `catch {}`.** By design, stated
  in its own docstring at line 4: "This is a probe, not a check. It asserts nothing." Confirmed
  nothing in `package.json` or `.github/workflows/ci.yml` invokes this script or reads its exit
  status — it has no npm script entry at all and is driven only by hand or by an external battery
  script per its own usage comment. Sound; not a finding.
- **`surface/mod.rs`'s seven unit tests covering `pixels()`/`pixels_over()`.** Both the Linux-only
  divisor path and the Windows-only pass-through path are exercised directly by the test module
  regardless of target OS (the `#[cfg(test)]` block is not gated by platform), so
  `#[cfg_attr(target_os = "linux", allow(dead_code))]` on `pixels()` and the Windows equivalent on
  `pixels_over()` suppress a real dead-code warning in non-test builds without leaving either
  function actually unexercised by `cargo test --workspace`. Not a finding; this is the "the right
  finding may be consolidate, not delete" case running in reverse — nothing to consolidate here, the
  duplication risk this brief asks about (does anything catch a regression in the half a platform
  doesn't call) is answered "yes, the unit tests, on both platforms, in the same file."
- **`close-gate-check.js` running 12 `check()` calls after the argv rewrite.** Counted directly:
  lines 261, 270, 275, 283, 291, 296, 301, 322, 330, 335, 350, 357 — twelve `check(` calls, matching
  `EXPECTED_CHECKS = 12`. The rewrite (typed-path setup replaced by an argv launch, `2000+1500` ms
  split into one `3500` ms wait) removed setup steps and changed wait _durations_, not any `check()`
  call. Whether the new single 3500 ms wait still lands the double-click after the cue list paints is
  a real question, but it is L1's ("does the double-click still land") — not a test-honesty question,
  since if it does not land, `doubleClickAt` hitting empty space would leave the document clean and
  `waitForDialog` would time out and throw, failing the run loudly rather than passing wrongly. So
  the check count survives the rewrite and the failure mode of a bad wait is a loud one, not a silent
  pass.
- **`e2e/README.md` for a description of a harness that no longer exists.** The spec table's row for
  `video-surface.spec.js` ("brings the picture back... the picture back") now describes pixel-level
  behavior the current assertion (map state + mpv child window) does not itself measure — this is the
  same substance as finding 4, so I am not double-counting it; it is the same disclosure gap surfacing
  in a second document. Beyond that, I did not find a description of tooling or a flow that has been
  physically removed (e.g., no reference to `SUBTITLE_PATH_FIELD`/`SUBTITLE_OPEN_BUTTON`, the deleted
  typed-path constants, survives in the README). The README's own `EXPECTED_TESTS = 30` code sample
  (in its "anti-zero-test guard" section) is stale against the real value in `wdio.conf.js`
  (`EXPECTED_TESTS = 33`) — but `git diff f0b0058 eca9806 -- e2e/wdio.conf.js e2e/README.md` shows
  neither `EXPECTED_TESTS` line changed in this range, so that specific staleness predates the gate
  and is out of scope per this gate's own rule (`f0b0058` and earlier is gate 1's). Not filed here;
  flagging for BACKLOG is the correct next step, done by whoever owns gate follow-up, not by widening
  this lens's scope.

## Note on the folded lens's remaining items

The "orphaned harness code and unguarded fixes" items not covered above — the `startup_files` path,
the NVIDIA mitigation, and `dialog.rs` each having "no check CI runs" — were traced but land as the
same underlying fact already reported under other lenses' primary ownership (L7 owns the NVIDIA
mitigation's unmeasured-and-possibly-never-run status in full; L6 owns `dialog.rs`'s thread safety,
where the relevant question is not test coverage but whether the code is sound at all; L8 owns
`startup_files`' error handling). Re-filing them here as coverage gaps without those lenses' fuller
context would fragment one finding into two half-findings across two reports. I confirmed the
narrower, unique-to-this-lens fact for each instead: `startup_files_command` (`lib.rs:62`) has no
dedicated E2E check anywhere in the range's added scripts (`close-gate-check.js` exercises it only
incidentally, by launching the app with a file argument, never asserting on `startup_files`'
classification logic directly — malformed-extension and multi-file argv cases have zero automated
coverage anywhere in the suite). This is real and worth the register knowing about, but I am
reporting it here as a fact for L8's fuller argument rather than a second finding, since L8 already
owns `startup_files` as an input-validation boundary and duplicating severity language across two
reports for one gap serves nobody.
