# N2b real-session check — 2026-08-30

Run against the owner's live session: Fedora, KWin Wayland, XWayland `:0` rootless, NVIDIA RTX 5070 Ti on driver 610.57.04, primary output 3840x2160 at fractional scale 1.5.

Two questions were answered and two new conflicts opened. Nothing is merged.

## 1. The blank window: found, fixed, verified

Launched on the real session the app opened a window that painted **nothing at all**, with `Failed to create GBM buffer of size 1024x700: Invalid argument` on stderr. WebKitGTK cannot allocate through the DMABUF path on this driver.

The documented escalation was measured rather than assumed, capturing the window directly with `import -window` — no synthetic input at any point:

| mitigation                           | window luma range | GBM error |
| ------------------------------------ | ----------------- | --------- |
| none                                 | 46..46, flat      | yes       |
| `__NV_DISABLE_EXPLICIT_SYNC=1` alone | 46..46, flat      | **yes**   |
| both variables                       | 16..235           | no        |

**The first step alone does nothing here**, contrary to what upstream expects on other driver versions. Only turning off the DMABUF renderer brings the interface back.

`main.rs` now applies both before the webview exists, keying the second on `/sys/module/nvidia`. Verified on the real display: three launches out of three render the full shell with no external variables set.

## 2. N2b's own question: answered, the frame is there

With the app rendering and the fixture passed as a command-line argument, on the real session:

| run | mpv child windows | surface    | saturation, empty shell → video open |
| --- | ----------------- | ---------- | ------------------------------------ |
| 1   | 1                 | IsViewable | 2.1 → 5.86                           |
| 2   | 1                 | IsViewable | 2.1 → 5.86                           |
| 3   | 1                 | IsViewable | 2.1 → 5.86                           |

Confirmed by eye as well: colour bars on screen. **The Xvfb flakiness was the software rasteriser's, not the app's.** The `gpu-context=x11egl` fix does what it was written to do.

`wayland-attach-check.js` is narrowed to the attachment accordingly, with that reasoning written into it: the attachment is what fails deterministically when the fix is removed, and the drawing is covered by `video-surface.spec.js` on a display where pixels can be trusted.

## 3. New defect found on the way: the surface is misplaced under fractional scaling

The video plays, but **not where it belongs**. On this display the native surface lands at 395x120 over the transcription bar instead of covering the stage, overlapping the controls. `VideoStage` reports a CSS-pixel rectangle and `apply_region` multiplies it by `window.scale_factor()`; at scale 1.5 the result is wrong in both position and size.

Not filed as part of N2b. It needs its own task, and it is user-visible on the primary platform.

## 4. Two conflicts I cannot resolve alone

**The mitigation breaks the ASR spec under Xvfb.** Isolated by removing it and putting it back: with the workaround, `asr.spec.js` fails four checks on 30-second timeouts and the file takes two minutes; without it, five passing in ten seconds. The detection keys on the NVIDIA module being loaded, which is true on this machine even under Xvfb, where rendering is llvmpipe and the workaround is not needed.

I do not know **why** ASR fails with it — slowness is my guess, not a measurement, and shipping either way without knowing is the wrong call. Options:

- **Understand the failure first.** If the workaround genuinely breaks something rather than slowing it, that matters on the real machine too, and the recommendation is to find out before choosing.
- Detect more precisely, i.e. whether NVIDIA is driving _this display_ rather than merely installed. No cheap way found: it needs a GL context or a subprocess before the webview exists.
- Apply it only in release builds. Simple, and it makes production code aware of its test environment, which the project has avoided so far.

**`video-surface.spec.js` is intermittent under Xvfb.** Its `before` hook waits for a picture, which is exactly the presentation that the probe measured as unreliable there: two of three recent runs pass. This is a fragility in a test written last night, not a regression from today. The real-session evidence above says the app is fine; the test's precondition is not dependable on that display.

## Method notes, paid for in failures

- On a live compositor `xdotool` typing goes to whichever window holds the X focus, and it landed **in the owner's own window** during the first attempt. Hence today's rule: synthetic input only inside isolated servers; on the real display only launch, arguments, screenshots.
- `x11grab` on this display reads black whatever the app does, because XWayland is rootless. `import -window <id>` captures the window directly and needs no raise, no focus and no compositor screenshot.
- The window does not keep the 1024x700 it asks for; it settles at 2151x1236, so the harness's geometry-based finder cannot see it here.

## 5. The two conflicts, resolved (2026-08-30, after the owner's ruling)

**The mitigation does not break ASR; it slows the app down.** Measured rather than guessed, within the hour the owner allowed. At X level nothing moves: the window appears in 260 ms and mpv attaches in 1314 ms with the workarounds and without them. The DOM is healthy too, `browser.execute` answering in 4–6 ms either way. What changes is how long a keystroke takes to reach React state: **373 ms with the workarounds, 186 ms without**. `asr.spec.js:149-155` clicks Open immediately after `typeText` without waiting for the field to hold the value, so it loses that race and only that race.

Cured with an escape hatch rather than a test-aware branch in production code: `SUBLORE_WEBKIT_WORKAROUNDS=0` turns the workarounds off, the harness sets it because under Xvfb the renderer is llvmpipe and a loaded NVIDIA module says nothing about what is drawing, and a user on a driver these workarounds hurt can turn them off without rebuilding.

**`video-surface.spec.js` no longer waits for a picture.** Under Xvfb the frame appeared 2 times in 10 while mpv was attached all 10, and the same mpv driven from the command line in the same Xvfb drew every time: the flakiness is llvmpipe's. The spec now asserts map state and mpv's child window, and the drawing is proved on real hardware instead, three runs out of three. Three consecutive full suites after the change: 8 spec files green each time, 57 seconds.

**`wayland-attach-check.js` no longer types.** It passes the fixture as a command-line argument, which is what `startup_files` was built for, so the input race above cannot reach it. The check discriminates, and that was measured: delete `gpu-context=x11egl`, rebuild, and it stops at "mpv attached its own window inside the surface" with the surface childless. mpv on its own shows the same shape under this Xvfb — `--wid` with `gpu-context=auto` leaves the host window with no children, with `x11egl` it gains one.

**A method note, paid for twice.** The E2E binary must be built after everything that compiles Rust or the frontend: `cargo test` and `cargo clippy --all-targets` overwrite `target/debug/sublore` with a plain cargo debug build that looks for the Vite dev server, and the close gate then fails with "Could not connect to localhost" on screen. And a build guarded by `&&` that fails prints nothing: the first attempt to prove this check discriminates ran against the old binary and reported the opposite of the truth. Both are now rules in WORKFLOW.md 4c.
