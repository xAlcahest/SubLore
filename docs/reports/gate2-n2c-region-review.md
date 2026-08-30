# Gate 2 — L11: N2c, the region contract as a public interface

Lens L11 of the gate 2 review (`docs/reviews/gate-2-plan.md` §2). Scope `GATE_BASE=f0b0058` …
`GATE_HEAD=eca9806`, read with git. Model: opus. Report written before terminating, per WORKFLOW §4b.

**Platform of every verdict below: Linux, by reading.** Nothing here was run. The battery at
`GATE_HEAD` was green (`docs/reports/gate2-battery-baseline.md`) and I did not re-run it; no finding
below claims the suite is red.

---

## 1. What I checked

**Code, whole files at `GATE_HEAD`:** `src-tauri/src/video/surface/mod.rs` (including all seven unit
tests), `surface/linux.rs`, `surface/windows.rs`, `src-tauri/src/video/mod.rs`,
`src/components/VideoStage.tsx`, `src/types/video.ts`, `src/hooks/useVideoPlayer.ts`.

**Diffs:** `git show c7261a5` in full (the N2c delivery), `git diff f0b0058 eca9806` restricted to
the region-contract files, `git show f0b0058:src-tauri/src/video/surface/mod.rs` for the `logical()`
/ `physical()` shape N2c replaced.

**Harness:** `e2e/scripts/scaled-surface-check.js` line by line, `e2e/lib/proc.js` (`waitFor`'s
truthiness contract, because three assertions in the new script depend on it), `e2e/lib/env.js`,
`e2e/lib/paths.js`, `e2e/lib/x11.js` (`findToplevel`, `findWindowsWithAppGeometry`),
`e2e/specs/video.spec.js` (the pre-existing geometry assertion, which is a consumer of the contract
nobody listed), `e2e/specs/video-surface.spec.js`, `e2e/README.md`.

**Wiring:** `package.json`, `.github/workflows/ci.yml` in full — both the `check` matrix and the
`e2e` job — to establish what runs where.

**Documents:** BACKLOG N2c (lines 98-105) and its three ACs, `docs/reports/n2c-p3-scala.md` in full,
`docs/design/decisions.md` (§14, added in range), `docs/design/x11-vs-render-api.md` §2 and §6,
plus a repo-wide grep for every citation of `surface/mod.rs`, `surface/linux.rs`, `VideoStage.tsx`,
`logical()` and `physical()`.

**Dependency sources, read rather than trusted** (plan §2, shared material):
`~/.cargo/registry/src/*/tao-0.35.3/src/platform_impl/linux/window.rs:54,362-369,431` and
`monitor.rs:49` — the report's `AtomicI32` / `as f64` argument is exactly what the source says, so I
did **not** contradict it (this lens's named false positive: avoided, and checked before avoiding);
`gdk-0.18.2/src/auto/window.rs:561-563` (`scale_factor` is a bare FFI passthrough to
`gdk_window_get_scale_factor`, so the value is a GDK integer ≥ 1 and `f64::from` of it is always
finite) and `:815-819` (`move_resize` is a bare passthrough to `gdk_window_move_resize`).

**Contract movement, item by item.** Every consumer named in the brief moved inside `c7261a5`:
`VideoStage.tsx` (multiplies), `types/video.ts:4,7` (unit restated), `video_set_region` (signature
lost its `WebviewWindow`), `surface/mod.rs` (`pixels`/`pixels_over` replace `logical`/`physical`),
`linux.rs:65` (`pixels_over(scale_factor())`), `windows.rs:59` (`pixels()`). Two consumers did
**not** move and are findings 4 and 5. One consumer nobody listed, `e2e/specs/video.spec.js:105-111`,
did not need to move: it derives the expected X rectangle from the DOM independently
(`Math.round(css * dpr)`) and still agrees with the app on both paths, within its 2 px tolerance.

---

## 2. Findings, most severe first

### F1 — serious — `e2e/scripts/scaled-surface-check.js:148` — an assertion that cannot fail, inside the counter built to stop exactly that

```js
check("the app came up at both ratios", single.surface !== null && double.surface !== null);
```

**How it fails.** `measureAt` cannot return with a null `surface`: line 94 is
`await waitFor(() => surfaceOf(toplevel), …)`, and `waitFor` (`e2e/lib/proc.js:13-32`) returns only
a truthy value and otherwise throws on its deadline. So both operands are guaranteed true by the
code that produced them. Worse, lines 140-146 already dereference `single.surface.width` and
`double.surface.width` in the two `console.log` calls _before_ the check runs, so a null would have
thrown a `TypeError` one statement earlier and this line would never be reached. There is no input,
state or sequence that makes this check print anything but `ok`.

It is not harmless. `EXPECTED_CHECKS = 5` (`:38`) exists so that gutting an assertion is as red as
failing one (`:37`, `:189-194`); one of those five is a constant, so the guard reports 5/5 while
protecting four. That is the defect WORKFLOW §2.5b names verbatim — "any check whose condition the
test itself guarantees is banned: it inflates the count that exists to catch removed assertions" —
and which the file's own header comment claims to be defending against. The baseline battery
(`gate2-battery-baseline.md:18`) records "scaled surface check passed (5/5 checks)", so the inflated
number is already on the record as evidence.

**Recommended correction.** Delete the check and set `EXPECTED_CHECKS = 4`, or replace it with
something the run can actually contradict — for example that the surface is a _direct_ child of the
toplevel at both ratios, or that exactly one large child exists at both (the leak assertion
`video-surface.spec.js`'s `surfaceWindow` makes and this script does not: `surfaceOf` at `:62-68` silently sorts
and takes the biggest, so a second surface would be hidden rather than reported).

### F2 — serious — `.github/workflows/ci.yml:196` (end of the `e2e` job) with `package.json:19` — the only automated check that can catch the removal of N2c's fix is run by no CI job

**How it fails.** Delete `* ratio` from `src/components/VideoStage.tsx:34-40` — i.e. revert N2c's
actual fix — and push. Everything CI runs stays green:

- `cargo test --workspace` (`ci.yml:123`, on both ubuntu and windows): every `surface/mod.rs` unit
  test operates on numbers the _test_ pre-multiplied, so none of them observes the page at all.
- `pnpm e2e` under Xvfb (`ci.yml:190`): `devicePixelRatio` is 1 there, so the deletion is a no-op —
  `video.spec.js:105-111` computes `round(css * 1)` and matches.
- `e2e:shutdown`, `e2e:close-gate`: unrelated.

The one thing that does catch it is `pnpm e2e:scale`. Traced: at `GDK_SCALE=2` the CSS layout is
identical at both ratios, so with the page multiplication gone the surface would measure the same
rectangle in both runs and `doubles(single.surface.width, double.surface.width)` (`:126-128`,
asserted at `:162-169`) fails with the message that names BACKLOG N2c. It also catches the reverse
regression the delivery says it was written for (dropping `pixels_over`'s divisor at `linux.rs:65`
gives 4×, `|4C − 2C| > 3`). That script is in `package.json` as `e2e:scale` and appears nowhere in
`.github/workflows/ci.yml` — the `e2e` job's last step is `e2e:close-gate` at `:194-196`, and the
file ends there. It runs only when a human remembers to type it. WORKFLOW §4a's "green full battery"
between gates is a local ritual for this check, not a CI gate; the next person to touch
`VideoStage.tsx` gets no signal.

**Recommended correction.** Add a step to the `e2e` job:
`xvfb-run -a -s "-screen 0 1280x1024x24" pnpm e2e:scale`. It needs nothing the job does not already
install (it uses `xwininfo` from `x11-utils` and `python3-xlib`, both present) and it is the only
guard the fix has.

### F3 — serious — `src/components/VideoStage.tsx:31` with `src-tauri/src/video/surface/linux.rs:65` — nothing re-reports the region when only the ratio changes, and the two halves of the ratio are read from two different places at two different times

**How it fails.** `const ratio = window.devicePixelRatio` is read once per `report()`, and `report()`
is scheduled from exactly three places (`:44-53`): the `ResizeObserver` on the element, the window
`resize` event, and mount. There is no `matchMedia("(resolution: Ndppx)")` listener — grepped, there
is no `matchMedia` and no second `devicePixelRatio` read anywhere in `src/`. So the region is
re-sent only when the element's CSS box changes or the viewport's CSS size changes.

The reachable sequence: with a video open, change the desktop scale factor while the app is running
(the XSETTINGS `Gdk/WindowScalingFactor` an X settings daemon pushes, which is the same integer
`GDK_SCALE` seeds — tao itself subscribes to it at `tao-0.35.3/src/platform_impl/linux/window.rs:364`,
`window.connect_scale_factor_notify`, so it is not a hypothetical value). Under an integer scale
factor the CSS viewport size is invariant: `scaled-surface-check.js` depends on precisely this — at
`GDK_SCALE=1` and `GDK_SCALE=2` the toplevel doubles in X while the page's CSS rectangle is the same,
which is why its `doubles()` assertion works at all. So the `ResizeObserver` does not fire, the
window `resize` event does not fire, and `report()` never runs again. The backend keeps the last
rectangle it was given at the old ratio.

Whether the picture then lands correctly depends entirely on GDK re-applying the new factor to a
manually created, `ensure_native()`'d **child** window's stored geometry — a behaviour nobody in this
repo has measured and which I could not establish from `gdk-0.18.2`, since both `scale_factor()` and
`move_resize()` are bare FFI passthroughs (`src/auto/window.rs:561-563`, `:815-819`) and the GTK3 C
side is not in the tree. If GDK does re-apply it, the geometry survives by luck rather than by
design; if it does not, the surface stays at the old X rectangle under a toplevel that just doubled,
which is N2c's own symptom — the video at a fraction of the stage — reached by a different route.

This is also the one place the design's own claim does not hold. `surface/mod.rs:19-20` says "The
ratio never travels with it: one number, one owner." It does not travel on the wire, but it is still
held twice: `devicePixelRatio` in the page and `gdk::Window::scale_factor()` on the child surface
(`linux.rs:65`), read at two different moments, and the result is correct only while they agree. I
tried and failed to construct a _static_ configuration where they disagree — `GDK_SCALE=2` alone,
KDE fractional 1.5 alone, and both together all come out right — so the design is sound at rest. It
is the transition that is unguarded, and the comment reads as though there were nothing to guard.

**Confidence: likely** for "the region is never re-reported" (that half is proved by reading:
three schedulers, none of which observes the ratio). **Suspicion** for the resulting misplacement,
which depends on the unmeasured GDK behaviour above.

**Recommended correction.** In `VideoStage.tsx`, subscribe to the ratio itself and `schedule()` on
it: `window.matchMedia(\`(resolution: ${window.devicePixelRatio}dppx)\`)`with a one-shot`change`listener re-armed after each fire (the standard idiom, since the query has to be rebuilt at the new
ratio). Then soften`surface/mod.rs:19-20` to say what is true: the ratio does not cross the IPC
boundary, but each platform re-reads its own half locally and the two must agree.

### F4 — minor — `src-tauri/src/video/mod.rs:90` — the Rust side of the contract still documents the old unit

```rust
/// A rectangle measured by the frontend with `getBoundingClientRect`, in CSS pixels.
pub struct VideoRegion { … }
```

Confirmed, exactly as the plan's §2b registered it before any lens ran. `src/types/video.ts:4,7` was
changed to "Native device px" in the same commit and its header says "changing either side means
changing both" (`types/video.ts:1`); `c7261a5` touched `video/mod.rs` (16 lines) and left this doc
comment describing the unit N2c abolished. `video/mod.rs:2` declares this module's payloads a public
interface under CLAUDE.md §6. A reader who trusts the Rust side and multiplies again gets the 1.5²
bug.

I treated it as the plan asked — as a starting point, not a result — and it led directly to F5 and
F7, which are the same defect class in the documents.

**Recommended correction.** "A rectangle already resolved to native device pixels by the page; see
`surface::SurfaceRegion` and `src/types/video.ts`."

### F5 — minor — `docs/design/x11-vs-render-api.md:25`, `:35`, `:153` — the adopted decision-14 document still describes the pre-N2c contract, and M2.0 is told to read it

**How it fails.** `docs/design/decisions.md:9-15` (added in this range) adopts this file as decision
14 and says the reasoning and the reversal conditions "are in `x11-vs-render-api.md`". BACKLOG:123
makes reading it a pre-M2.0 obligation. At `GATE_HEAD` three of its load-bearing sentences describe
code that no longer exists, and `c7261a5` did not touch the file:

- `:25` — "`VideoStage` measures `.stage__surface` with `getBoundingClientRect` **in CSS pixels**".
  False since `VideoStage.tsx:34-41`.
- `:35` — the N2c mechanism section, present tense: "`set_region` calls `region.logical()`, which is
  `scaled(1.0)`", citing `surface/mod.rs:44-47`. `logical()`, `physical()` and `scaled()` were all
  deleted in `c7261a5`; `surface/mod.rs:44-47` is now the body of `pixels_over`. It also concludes
  that N2c's second AC "would pin arithmetic the Linux path does not reach", which the delivery then
  made false — the Linux path reaches `pixels_over` with a real divisor.
- `:153` — under "Three pieces of work should happen regardless", the live action item "**N2c needs
  P3 before it needs code**", prescribing a test that asserts "`logical()` is scale-independent" and
  "`physical()` scales". Both functions are gone and N2c has shipped. An open action list telling
  M2.0 to do work that is done, against an API that does not exist.

**Recommended correction.** Date-stamp §2's N2c paragraph as the pre-fix diagnosis and point it at
BACKLOG N2c's status line; fix `:25` to say native device pixels; strike or close the `:153` bullet.

### F6 — minor — `src-tauri/src/video/surface/linux.rs:65` — the divisor path rounds each side independently, which is the thing `VideoStage.tsx:32-33` says it was written to prevent

**How it fails.** `VideoStage.tsx:32-33` states the invariant: "Edges first, then the size from them,
so a rectangle never gains or loses a pixel to rounding each side independently." `pixels_over`
(`surface/mod.rs:51-66`) then divides and rounds `x` and `width` as two independent values, so on the
divisor path the invariant is discarded one function later.

Concretely, at `GDK_SCALE=2` (`devicePixelRatio` 2) with a stage at CSS `left = 288.25`,
`right = 800.75`: the page sends `x = round(576.5) = 577`, `width = round(1601.5) − 577 = 1025`.
`pixels_over(2.0)` gives `position(577) = round(288.5) = 289` and `size(1025) = round(512.5) = 513`,
and GDK multiplies both by 2, so X receives `x = 578`, right edge `1604` against a correct
`577 … 1601.5`. One native pixel on the left, two and a half on the right — the surface is one
logical pixel wider than the stage and offset by one.

The magnitude is cosmetic and `video.spec.js`'s 2 px tolerance absorbs it, which is why this is
minor and not serious. It is reported because a comment claims an invariant the code does not keep,
which CLAUDE.md §9 treats as a claim, not a style question.

A second, smaller disagreement sits in the same place and I could not make it bite: `Math.round`
rounds halves toward +∞ (`Math.round(-2.5) === -2`) while Rust's `f64::round`, pinned by
`surface/mod.rs:169-172` (`halves_round_away_from_zero`), rounds them away from zero. The two never
meet today, because the page's output is already integral so Rust's rounding is the identity on the
`divisor == 1` path, and a negative coordinate at an exact half needs both `GDK_SCALE ≥ 2` and a
stage laid out off the left edge of the viewport, which the current shell cannot produce. It becomes
reachable the moment M2.0 gives a panel a horizontal offset. **Suspicion**, recorded so the divisor
path is not assumed to be sign-symmetric.

**Recommended correction.** Have `pixels_over` derive the size from the divided edges the way the
page does — `size = position(x + width) − position(x)` — so both sides of the boundary use the same
rule, and say so in the comment.

### F7 — minor — citation drift into `surface/mod.rs`, caused in range and not fixed in range

**How it fails.** `c7261a5` added 146 lines to `src-tauri/src/video/surface/mod.rs` and updated no
citation to it. `docs/design/decisions.md:21` cites `surface/mod.rs:80` for "Move, resize and raise
above the webview" — line 80 is now `impl VideoSurface {`, the doc comment is at `:93`. The
`show()`-must-precede-mpv comment cited as `surface/mod.rs:82-84` in `decisions.md:29`,
`docs/reports/n2-probe.md:7` and `docs/design/post-v1-plan.md:258` is now at `:98-100`. `linux.rs:66`,
cited for `raise()` in `decisions.md:21` and `docs/design/shell-layout.md:141`, is now `move_resize`;
`raise()` moved to `:67`.

This is the same class BACKLOG:123 already declares a pre-M2.0 blocker ("M2.0 is designed against
line numbers that moved"), and this delivery added to the pile rather than reducing it. Minor on its
own; it belongs in the same fix as F5. **Overlaps L12**, which owns citation resolution repo-wide —
recorded here because the drift was caused by the file this lens owns, and because the plan's dedup
rule collapses same-site findings rather than double-counting them.

### F8 — minor — `src-tauri/src/video/surface/mod.rs:43`, `:51` — `pixels()` and `pixels_over()` were widened to `pub` with no consumer that needs it

**How it fails.** The methods they replace were private (`git show f0b0058:…/surface/mod.rs:45`,
`:51`: `fn logical`, `fn physical`) and were called from `linux.rs` / `windows.rs` all the same,
because `mod platform` is a descendant of `surface` and Rust privacy is visible to descendants. The
same is true of `#[cfg(test)] mod tests`. Nothing outside `surface` calls either method — `video/mod.rs`
only constructs `SurfaceRegion` and calls `VideoSurface::set_region`. CLAUDE.md §6 asks for the
minimum that solves the problem; this widens the exported surface of a type whose module comment
(`surface/mod.rs:23-24`) says the per-platform arithmetic "stays behind this type".

**Recommended correction.** Drop both `pub`. The `#[cfg_attr(…, allow(dead_code))]` attributes stay
correct either way, since `mod surface` is private in `video/mod.rs:6`.

### F9 — minor — `BACKLOG.md:104` and `docs/reports/n2c-p3-scala.md:73` — two evidence claims that the durable record does not carry

Two separate small honesty gaps in the same status block. Neither is a fabrication; both are things
the commit message says and the record does not.

**(a) The Windows half is not labelled where the record lives.** `windows.rs:59` changed behaviour —
`physical()` (multiply by `scale_factor()`) became `pixels()` (take as given) — and no Windows run
exists. `c7261a5`'s commit message says so plainly ("Windows compiles in CI and has never had these
checks run against it"), but `BACKLOG.md:104`, the line anyone reads later, opens "Status 2026-08-30,
verified **on the owner's 1.5 display** and by tests on Linux" and then states "Windows takes the
numbers as they are" as settled fact with no platform mark on it. CLAUDE.md §9 puts the platform on
every behavioural verdict; the plan's own "does not count" clause exempts a missing Windows run only
_provided it is labelled_, and in the durable record it is not. **Recommended correction:** append
"Windows compiles in CI; the Windows path has never been run" to the status line.

**(b) The screenshot AC's evidence names no method and retains no artefact.** AC1 (`BACKLOG.md:101`)
requires verification "by a screenshot of the app's own window". `n2c-p3-scala.md:73` says "A capture
of the window shows the picture inside the stage" — it does not name the command, and WORKFLOW §4c
exists precisely because the wrong one (`x11grab` on the root window) reads black under rootless
XWayland while `import -window <id>` does not. No image is committed anywhere in the repo (checked:
`git ls-files` returns only `src-tauri/icons/*`). The _measured_ half of that AC is sound and
re-checkable — `n2c-p3-scala.md:70-73` gives `surface 592x180 at +432+500` against a CSS rectangle of
394.67x120 at 288,333, and I recomputed it through the page's actual formula:
`round(288×1.5) = 432`, `round(682.67×1.5) − 432 = 592`, `round(333×1.5) = 500`, `round(453×1.5) − 500 = 180`.
It is only the picture half that rests on an unrepeatable sentence. **Recommended correction:** name
the capture command in the report, as `wayland-attach-check.js` and `real-session-check.mjs` do.

---

## 3. Hunt items checked and found sound

**The three ACs, against their evidence.**

- **AC1 (surface covers the stage at 1.5, measured against a screenshot).** The measurement is sound
  and I re-derived it independently through the page's own edge-difference formula — see F9(b). The
  numbers agree to the pixel, including the `round(499.5) = 500` half-up case. Only the screenshot
  half is unsupported, which is F9(b) and minor.
- **AC2 (a unit test pins the conversion, covering a fractional ratio and the 16-bit clamp, running
  in CI).** Half met, and I want to be precise about which half, because the wording invites a
  stronger reading than the tests support. The clamp is genuinely pinned and genuinely runs in CI:
  `coordinates_clamp_to_the_x11_limit` (`surface/mod.rs:174-189`) can fail (remove the clamp and the
  cast wraps), and `cargo test --workspace` runs on **both** ubuntu and windows in the `check` matrix
  (`ci.yml:14-18`, `:123`), so the Rust half is covered on both platforms — better than I expected
  and worth recording. The fractional ratio is a different story: no test anywhere applies one. The
  code never sees 1.5 — `a_fractionally_scaled_rectangle_passes_through` (`:132`) feeds it 1023,
  already multiplied by the test author, and `the_divisor_undoes_only_what_the_window_system_re_applies`
  (`:145`) writes `css * 1.5` in the fixture and then calls `pixels_over(1.0)`. The multiplication is
  in `VideoStage.tsx:34-41`, and the repo has no frontend test runner at all (`package.json` has no
  `test` script and no vitest/jest in `devDependencies`), so the page-side multiplication has no unit coverage and cannot get
  any without a new dependency. I did not raise this as a finding separate from F2 because the fix is
  the same one: the only automation that can discriminate the page-side multiplication is
  `e2e:scale`, and wiring it into CI closes both. `BACKLOG.md:104`'s "Unit tests cover 1, 1.5 and 2
  on both paths" should be read as "cover the pass-through and the divisor at 1 and 2"; the 1.5 is in
  the fixture, not in the code under test.
- **AC3 (the E2E suite green under Xvfb, where the change must be a no-op, and the delivery says a
  green suite there is not sufficient).** Met, and this is the honest part of the delivery.
  `scaled-surface-check.js:4-16` says it in the file itself ("**This does not prove N2c, and saying
  so is the point**"), `BACKLOG.md:104` repeats it, and `n2c-p3-scala.md:63` states it with the
  measurement behind it (Xft.dpi via xrdb and a gtk-xft-dpi settings file both leave
  `devicePixelRatio` at 1). Three places, consistent, no overclaim. The change _is_ a no-op at
  dpr 1: `ratio = 1` makes `VideoStage`'s multiplication the identity and `scale_factor()` is 1, so
  `pixels_over(1.0)` is `pixels()`.

**Every consumer moved in the same change.** Verified one by one against `git show c7261a5` — see §1.
The two that did not move are F4 and F5. A third candidate I expected to find broken is not:
`e2e/specs/video.spec.js:105-111` computes its own expected rectangle as `round(css * dpr)` and
compares to `xwininfo` within 2 px. Its comment ("X11 geometry is physical; the DOM rect is in CSS
pixels") remains true of what it itself does, and its arithmetic agrees with the app on both paths —
fractional (page multiplies, backend divides by 1) and integer (page multiplies, backend divides by
N, GDK multiplies by N). It even absorbs F6's one-pixel edge disagreement inside its tolerance.
`e2e/README.md:54` is likewise still accurate.

**The two `#[cfg_attr(…, allow(dead_code))]` attributes, and what catches a defect in the half a
platform does not compile against.** Both halves are exercised by the unit tests, and the tests run
on both platforms (`ci.yml:123`, `cargo test --workspace` in the windows matrix leg), so the answer
to "what catches it" is: the unit suite, on the platform that does not use the function. `pixels()` is
asserted on at `:134-137`, `:157`, `:166`, `:171`, `:179` and `:195`; `pixels_over()` at `:150`,
`:154` and `:166`. Both can fail for a constructed cause —
ignore the divisor and `:154` breaks, drop the clamp and `:179` breaks. Sound, and the `cfg_attr`
spelling is correct: `target_os = "linux"` on `pixels`, `windows` on `pixels_over`, matching which
platform calls which.

**`a_nonsense_divisor_is_ignored_rather_than_applied` — what counts as nonsense.** The guard is
`divisor.is_finite() && divisor >= 1.0`, so nonsense is: zero, negative, anything in (0, 1), NaN and
both infinities. The test asserts four of those (`0.0`, `-2.0`, `0.5`, `NaN`) and not `f64::INFINITY`
— which the guard does handle, via `is_finite()`, so this is an untested branch and not an unhandled
one. Not raised as a finding: the assertion loop can fail for a real cause, and the missing case
costs one array element whenever someone next touches the file. On what `scale_factor()` actually
returns on a confused display: `gdk-0.18.2/src/auto/window.rs:561-563` is a bare passthrough to
`gdk_window_get_scale_factor`, which returns the X11 impl's `window_scale`, an integer GDK never
lets below 1, so `f64::from(i32)` is finite by construction and the guard is defence in depth rather
than a live path. Correct to keep it, per CLAUDE.md §6's boundary rule.

**`sizes_never_reach_the_window_api_as_zero` versus `types/video.ts:7` "Zero in either dimension
hides the surface" — two invariants, not a contradiction.** Traced through `apply_region`
(`video/mod.rs:263-276`): `is_empty()` (`surface/mod.rs:68-72`) is evaluated first, and `set_region` is called only when the
region has area, so the floor-at-one in `pixels_over` (`surface/mod.rs:59`) is reachable in
production only when the _divisor_ shrinks a genuinely non-empty rectangle below one pixel, never as
a way of turning a zero into a one. `types/video.ts:7` describes the IPC contract, the Rust floor
describes what a window API will accept, and `settle` (`video/mod.rs:64-88`) is still the single
place that decides visibility — N2c did not touch the derived-visibility machine N2 built, and
`region_empty` is still written from the same `is_empty()` the geometry branch tested.
`an_empty_region_is_recognised_before_it_is_converted` (`:199-205`) pins both dimensions and the NaN
case and can fail for each. Sound.

**The 16-bit X11 clamp at 1.5 on a large display.** Not reachable on the hardware in question and not
close to it: the owner's 3840x2160 at 1.5 gives a CSS viewport of 2560x1440 and native coordinates
bounded by 3840, an order of magnitude under `i16::MAX = 32767`. To reach the clamp through
`devicePixelRatio` you would need roughly a 32k-pixel-wide native surface. The clamp's real job is
defending against a garbage rectangle rather than a large display, and `coordinates_clamp_to_the_x11_limit`
pins it. One behaviour worth writing down rather than filing: at the boundary the clamp is applied to
position and size independently (`:57-58`), so a rectangle straddling the limit is silently squared
off rather than rejected — correct for X, which would wrap the cast, and unreachable today.

**The open suspect — 800x600 against a logged inner size of 1024x700 — survives on the record.** It
was not quietly dropped and it was not absorbed by the geometry work. It is carried in two places:
`n2c-p3-scala.md:40` (where it was found) and `:75` (where the later launch "**did not reproduce**"
and it is explicitly kept open, "because one clean launch is not an explanation"), and
`BACKLOG.md:105` as a standing "Open suspect" bullet on the N2c entry. The plan's harness worry is
real and I confirmed the mechanism: `e2e/lib/paths.js:22-23` freezes `windowWidth = 1024` /
`windowHeight = 700` and `e2e/lib/x11.js:74-88` `findToplevel` matches on exact equality, so a window
that resized itself to 800x600 would make `findToplevel` return null and every spec time out on "the
1024x700 Sublore toplevel to appear" rather than on the real cause. That is a latent harness trap,
but it predates this range and no lens is entitled to re-file it as new; N2c's own script correctly
sidesteps it by matching on name instead (`scaled-surface-check.js:49-59`, with the reason written
down). Recorded as sound, with the note that if the suspect ever reproduces, the first symptom will
be a misleading timeout.

**The report's core mechanism, re-derived rather than taken on trust.** The plan warned that this
lens's most likely false positive is disagreeing with the conclusion that the multiplier must come
from the page. I checked the `tao` argument before deciding: `window.rs:431` is
`self.scale_factor.load(Ordering::Acquire) as f64` over the `Rc<AtomicI32>` declared at `:54` and
seeded at `:362` from `window.scale_factor()`, with `connect_scale_factor_notify` at `:364` keeping
it current; `monitor.rs:49` is the same `as f64` over an integer. The claim holds exactly as written,
1.5 is not a value that field can hold, and the page is therefore the only party that knows the full
ratio. I do not contradict it. The complementary half — that GDK re-applies the integer factor to
child geometry on the way to X, which is why `linux.rs:65` divides — I could not verify from source
(`gdk-0.18.2` is a passthrough and the GTK3 C side is not in the tree), but it is supported by the
measurement the delivery records (4× instead of 2× under `GDK_SCALE=2`) and by
`scaled-surface-check.js` passing at 5/5 in the baseline battery. Stated as measured, not as read.

---

_L11 complete. Nine findings: three serious, six minor. No blocker: nothing in N2c's diff can cost
the user data, their file, or crash the app on Linux — the whole change is window geometry, it
touches no write path, and `apply_region`'s failure mode is a misplaced rectangle._
