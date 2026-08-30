# Gate 2 — Wave 3 fixes: the video region contract and the mpv context

Cluster: `src-tauri/src/video/player.rs`, `src-tauri/src/video/mod.rs`,
`src-tauri/src/video/surface/mod.rs`, `src-tauri/src/video/surface/linux.rs`,
`src/components/VideoStage.tsx`, `src/types/video.ts`. No file outside that list was touched.

**Platform of every behavioural verdict below: Linux.** Two displays: the owner's real KDE Wayland
session on `:0` (`devicePixelRatio` 1.5) and an Xvfb the harness owns. Nothing here was run on
Windows; the Rust unit tests added below compile and run on both, but no Windows behaviour is
claimed.

Register rows taken: `player.rs:190` (serious, L1+L4), `VideoStage.tsx:31` (serious, L11),
`video/mod.rs:90` (minor, L11, pre-registered in the plan §2b), `surface/linux.rs:65` (minor, L11),
`surface/mod.rs:43` (minor, L11). All five are fixed. One test expectation changed and is named in
§6. What I could not prove is in §2; the run I could not explain is in §8.

---

## 1. `player.rs:190` — `gpu-context=x11egl` forced on every Linux launch — **fixed**

**What the row said.** The option is set for every `wid`-backed player on Linux, the comment names a
condition ("with a Wayland display in the environment") that the code never tests, mpv's own context
probing is removed for machines that never had the defect, and unlike the webview mitigation in the
same range there is no escape hatch.

**What I changed.** `Player::new` now decides through a pure function:

```rust
fn gpu_context_for(hatch: Option<&str>, wayland_display: Option<&str>) -> Option<String>
```

- no hatch, a Wayland display present → `x11egl`, exactly as before;
- no hatch, no Wayland display → nothing set, mpv probes as it did before `062f201`;
- `SUBLORE_MPV_GPU_CONTEXT=<name>` → that context (`auto` hands the choice back to mpv);
- `SUBLORE_MPV_GPU_CONTEXT=` (empty) → nothing set.

The decision is logged once per player, before mpv is created, because "which output path did the
app take" is the first question a "no picture" report needs and nothing recorded it:

```
[INFO][sublore_lib::video::player] video: gpu-context x11egl (SUBLORE_MPV_GPU_CONTEXT=unset, WAYLAND_DISPLAY=wayland-0)
```

**Why this condition and not another.** The probe behind the option
(`docs/reports/n2b-probe.md:13`) set `WAYLAND_DISPLAY` for **every** row, so its "none → not
attached" line was never a statement about a plain X11 session. I re-ran the same experiment today,
Xvfb `:96`, mpv 0.41.0, `--wid` on a python-xlib container window, counting mpv's own child window
with `xwininfo -children`:

| environment             | option                 | mpv child windows |
| ----------------------- | ---------------------- | ----------------- |
| `WAYLAND_DISPLAY` set   | none (`auto`)          | 0                 |
| `WAYLAND_DISPLAY` set   | `--gpu-context=x11egl` | 1                 |
| `WAYLAND_DISPLAY` unset | none (`auto`)          | **1**             |
| `WAYLAND_DISPLAY` unset | `--gpu-context=x11egl` | 1                 |

The third row is the one nobody had measured: without a compositor to take, mpv's own probing
already lands in the X11 window. So pinning EGL there removes the GLX fallback of a stack that has
no usable EGL-on-X11 and gives that machine nothing in exchange, which is the failure L1 described.

**What proves it.** Four unit tests in `player.rs` (`cargo test --workspace`, run on both platforms
in CI): the Wayland case pins `x11egl`, the no-Wayland case leaves mpv alone, the hatch forces a
context in both environments, an empty hatch hands the choice back. Delete the Wayland arm and
`a_wayland_display_pins_the_x11_context` fails; delete the guard and
`without_a_wayland_display_mpv_keeps_its_own_probing` fails.

Behaviourally, on the owner's real Wayland session: `pnpm e2e:wayland` **8 runs, 8 green** (4/4
checks each), each logging `gpu-context x11egl (… WAYLAND_DISPLAY=wayland-0)` — the option still
reaches mpv exactly where the defect lives. Under Xvfb, where the harness scrubs `WAYLAND_DISPLAY`
(`e2e/lib/env.js:29`) and the option is now **not** set, the full WebDriver suite is green (8 spec
files, including `video-surface.spec.js` and `video.spec.js`, which assert the surface is mapped,
carries mpv's own window and matches the DOM rectangle), and so is `pnpm e2e:scale` (5/5 checks; one run's exit-status caveat is in §7).

The hatch is documented in the code and in this report only. A user-facing place to write down
`SUBLORE_MPV_GPU_CONTEXT` and `SUBLORE_WEBKIT_WORKAROUNDS` together (README or `e2e/README.md`) is
neither of my files — noted in §8.

## 2. `VideoStage.tsx:31` — nothing re-reports the region when only the ratio changes — **fixed**

**What the row said.** `report()` is scheduled from three places, all of which observe geometry;
none observes the ratio. Under an integer scale-factor change the CSS box is invariant, so the
backend keeps the rectangle it was given at the old ratio.

**Measured first, because two things in the recommendation turned out to be wrong.** Xvfb `:97`,
`xsettingsd` as the XSETTINGS manager, the app driven through `tauri-driver` (launch and JS
evaluation only, no synthetic input). `Gdk/WindowScalingFactor` flipped 1 → 2 while the app ran,
with listener counters reset immediately before the flip:

| what                                        | before                     | after     |
| ------------------------------------------- | -------------------------- | --------- |
| `window.devicePixelRatio`                   | 1                          | 2         |
| `matchMedia("(resolution: 1dppx)").matches` | true                       | false     |
| `.stage__surface` CSS rect                  | 736 x 159.48 at 288, 295.9 | identical |
| `window` `resize` events                    | —                          | **0**     |
| `ResizeObserver` on `.stage__surface`       | —                          | **0**     |
| `(resolution: …dppx)` change events         | —                          | 1         |
| `(min-resolution: 1.5dppx)` change events   | —                          | 1         |

The first three rows confirm the row's premise on a live app: the ratio doubles, the CSS box does
not move. Rows four and five confirm the old code has no trigger. Rows six and seven are the fix's
mechanism firing.

The two corrections to L11's recommended idiom, both measured:

- **The query must not be built from `devicePixelRatio`.** On the owner's display
  `window.devicePixelRatio` is 1.5 while `matchMedia("(resolution: 1.5dppx)").matches` is **false**:
  the CSS `resolution` feature reports the device's own factor (1dppx there), and the fractional
  part of the ratio is page zoom. A query built at 1.5 is false before and after any integer change,
  so it never fires — on the one display the fix exists for.
- **`ResizeObserver` with `box: "device-pixel-content-box"` is not available here.** WebKitGTK
  throws `TypeError` on it.

**What I changed.** Three static threshold queries, half way between the integer factors a window
system hands out, so any move from one factor to the next flips exactly one of them:

```tsx
const RATIO_THRESHOLDS = [1.5, 2.5, 3.5];
const ratioQueries = RATIO_THRESHOLDS.map((t) => window.matchMedia(`(min-resolution: ${t}dppx)`));
for (const query of ratioQueries) query.addEventListener("change", schedule);
```

They feed the existing `schedule()`, so the ratio change is coalesced into the same one-per-frame
`report()` as everything else, and they are removed in the effect's cleanup. The page-zoom half of
the ratio needs nothing new: a zoom change alters the CSS viewport, so the existing `ResizeObserver`
and `resize` listener already fire and `report()` re-reads the ratio.

`surface/mod.rs`'s type comment claimed "The ratio never travels with it: one number, one owner",
which was the half of the design that was not true — each side reads its own half locally. It now
says that, and names the page's re-reporting as the reason the two agree.

**What proves it, and what does not.** The measurement above is the proof that the mechanism fires
where the old code was silent (remove the listeners and `thresholdChange` goes to 0, which is the
old code's `resize: 0` / `cssBox: 0`). The last link — listener fires, therefore `schedule()`,
therefore `invoke("video_set_region")` — is three straight-line statements in the same file, and I
could **not** instrument it: `window.__TAURI_INTERNALS__.invoke` is `writable: false,
configurable: false`, so a harness cannot wrap it (my first counter reported zero invokes for that
reason, not because the app was silent; the corrected run is the table above).

I also could not build a behavioural check that fails without this fix, and the reason is worth
recording rather than hiding: on Linux the consequence is invisible. GDK stores the child window's
geometry in logical pixels and re-multiplies by the new scale factor, so after an integer change the
surface lands on the same X rectangle whether the page re-reported or not. A fractional change moves
the CSS box, which the old listeners already caught. The case this fix actually protects is the one
the register row names — a window moved between a scaled and an unscaled monitor, i.e. per-monitor
DPI, which is Windows behaviour and unverified there by policy. So: fixed, mechanism measured,
consequence unobservable on Linux today. A committed check would have to live in `e2e/scripts/`,
which is another implementer's file this wave; §8 carries it as a BACKLOG line with the recipe.

No regression: the battery in §7 was run after this change.

## 3. `video/mod.rs:90` — the Rust side of the contract documented the old unit — **fixed**

`VideoRegion`'s doc said "measured by the frontend with `getBoundingClientRect`, in CSS pixels"
after N2c made the page resolve the rectangle to native device pixels. It now says native device
pixels, points at both other statements of the unit, and names N2c.

**The third place.** The brief asked me to treat the row as a starting point. `src/types/video.ts:1`
said "changing either side means changing both" — but there are three sides, not two:
`types/video.ts`, `video/mod.rs`'s `VideoRegion`, and `surface/mod.rs`'s `SurfaceRegion`, and the
one that drifted is the one the two-sided sentence does not count. That header now names all three.
(The fourth statement of the unit, `docs/design/x11-vs-render-api.md:25`, is a register row in the
docs cluster and was already corrected there in the working tree; I did not touch it.)

**What proves it.** Nothing automated: it is a comment, and I will not claim otherwise. It was
checked by reading all three statements side by side and re-deriving the arithmetic in
`n2c-p3-scala.md:70-73` through the page's formula (`round(288 × 1.5) = 432`,
`round(682.67 × 1.5) − 432 = 592`), which agrees with the unit as now documented.

## 4. `surface/linux.rs:65` — the divisor path rounded each side independently — **fixed in `pixels_over`**

The row's site is the call; the arithmetic is in `SurfaceRegion::pixels_over`
(`surface/mod.rs`), so that is where the change is. `linux.rs` itself is unchanged.

`pixels_over` divided and rounded `x` and `width` as two independent numbers, discarding one
function later the invariant `VideoStage.tsx:32-33` states it keeps. It now derives both edges and
takes the size from them, which is the page's own rule:

```rust
let edge = |value: f64| (value / divisor).round();
let span = |start: f64, length: f64| (edge(start + length) - edge(start)).clamp(1.0, COORD_LIMIT) as i32;
```

The clamps stay where they were: positions clamp to ±`COORD_LIMIT`, sizes floor at 1 and cap at
`COORD_LIMIT`, and the difference is taken on the unclamped rounded edges so a rectangle beyond the
16-bit limit still clamps to the limit rather than collapsing to nothing.

**What proves it.** A new test, `a_size_never_overshoots_the_edges_it_came_from`, with L11's own
example: at `GDK_SCALE=2` a stage the page reports as x 577, width 1025 has edges at 288.5 and 801,
so the surface is 512 wide, not 513. Discrimination run: with the old independent rounding restored,
that test fails `left: (289, 167, 513, 91)` against `right: (289, 167, 512, 90)`.
`pnpm e2e:scale` still measures the surface doubling exactly (736x159+288+296 at ratio 1,
1472x320+576+592 at ratio 2).

## 5. `surface/mod.rs:43` — `pixels()` and `pixels_over()` widened to `pub` — **fixed**

Both are private again. The only callers are `linux.rs` and `windows.rs`, which are descendants of
`surface` and see private items; `is_empty` stays `pub` because `video/mod.rs` calls it from
outside. The `cfg_attr(…, allow(dead_code))` attributes stay, one per platform, unchanged.

**What proves it.** The compiler: `cargo test --workspace` and `cargo clippy --all-targets` are
clean, so nothing outside `surface` needed the wider visibility. Add a caller in `video/mod.rs` and
the build fails.

---

## 6. Test changes, named (WORKFLOW §4)

One existing expectation changed: `halves_round_away_from_zero` asserted
`region(0.5, -0.5, 2.5, 3.5).pixels() == (1, -1, 3, 4)` and now asserts `(1, -1, 2, 4)`. The width
is the deliberate consequence of the change in §4: the rectangle spans x 0.5 … 3.0, so the nearest whole-pixel
window is 1 … 3, two wide. The old 3 was the independently rounded length, which hung a pixel past
the right edge. Nothing was weakened: the test still pins half-rounding on both positions and still
fails if the rounding rule changes (discrimination run above shows it failing under the old
arithmetic). No test was skipped, deleted or retargeted.

## 7. Verification run, in the order WORKFLOW §4c requires

Rust and frontend first, the E2E binary last, every exit status read rather than chained:

- `cargo test --workspace` — green, including the 4 new `player.rs` tests and the 8 `surface`
  tests. (An earlier run showed two failures in `dialog.rs`, another implementer's file mid-edit;
  they are green in the final run.)
- `cargo clippy --all-targets` — no warnings.
- `npx tsc --noEmit`, `pnpm lint`, `prettier --check` on my two frontend files — clean.
- `pnpm e2e:build` — exit 0, checked explicitly.
- `xvfb-run … pnpm e2e` — 8 spec files, all passed, exit 0.
- `xvfb-run … pnpm e2e:scale` — six runs today, five green at 5/5. One run failed its **last**
  check, the exit one: the first of the two apps left with `SIGSEGV` instead of status 0
  (`exits [{"code":null,"signal":"SIGSEGV"},{"code":0,"signal":null}]`). All four geometry checks
  passed in that run, and the measured rectangles are identical to the green runs
  (736x159+288+296 at ratio 1, 1472x320+576+592 at ratio 2). This is N1b, the known crash on the
  way out inside GDK's X event queue, documented at ~1 in 12 closes
  (`docs/reports/n1b-segfault-uscita.md:17,65`); a scale run closes two apps, so six runs is twelve
  closes and one crash. It is not caused by anything in this cluster, and I am not reporting the
  battery as unconditionally green because of it.
- `pnpm e2e:wayland` on the owner's real session — 4/4 checks, exit 0 (eight further runs, §8.1).

## 8. Noticed, not touched (WORKFLOW §4 — for BACKLOG)

1. **One unexplained run.** The first `e2e:wayland` run of this session failed: the toplevel
   appeared, the video opened (the app logged `gpu-context x11egl` and no error), and the native
   surface stayed at its creation size of 1x1 for the full 30 s, so no region was ever applied.
   Eight later runs on the same display were green, three of them on the identical binary, and the
   failing and passing logs are line-for-line identical. I could not reproduce it and I could not
   explain it. Its user-visible shape would be "the video does not appear, and nothing says why".
   Worth a BACKLOG line of its own; the log is preserved at
   `/tmp/claude-1000/-home-alcahest-git-SubLore/e7551c0d-c82d-4d53-9140-c45ee68deffa/scratchpad/wayland.log`.
2. **No committed check guards the ratio change** (§2). Recipe for one, all of it exercised today:
   Xvfb, `xsettingsd` writing `Gdk/WindowScalingFactor`, `SIGHUP` to change it live, the app under
   `tauri-driver`, and assertions on `window.devicePixelRatio`, on the element's CSS rect being
   unchanged, and on a listener counter. It belongs in `e2e/scripts/`, which is not my file this
   wave.
3. **`e2e/scripts/wayland-attach-check.js:146`** states that under this Xvfb `--wid` with
   `gpu-context=auto` "leaves the host window childless". Measured today: that holds only with
   `WAYLAND_DISPLAY` inherited from the developer's session. With it unset, `auto` attaches. The
   sentence is true of how it was run and misleading about what it proves.
4. **`docs/reports/n2b-probe.md`** has no row for a plain X11 environment, which is why the option
   was written unconditionally. The four-cell table in §1 is the missing measurement and could be
   appended there.
5. **Harness note worth writing down somewhere durable:** `window.__TAURI_INTERNALS__.invoke` is
   non-writable and non-configurable, so a check cannot count IPC calls by wrapping it, and a
   wrapper that looks installed will silently count zero. This cost me one wrong measurement.
6. **`ResizeObserver` `device-pixel-content-box` is unsupported** in this WebKitGTK — relevant to
   M2.0 if anything there wants device-pixel geometry from the page.
7. **A stray untracked file named `--help` sits in the repo root** (`git status`), almost certainly
   a mistyped command from some session. Not mine to delete, but it would be committed by a
   `git add -A`.
