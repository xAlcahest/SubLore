# N2b — stopped, 2026-08-30, not merged

The fix works. What cannot be proved under Xvfb is that the frame reaches the screen.

**On a branch, not on main.** `main` is clean at `f0b0058`.

## What is settled

libmpv did not attach to the X11 window it was handed whenever a Wayland display was in the environment: `gpu-context=auto` picked Wayland and drew past the `wid`. Pinning the context fixes that, and the probe measured four options that all attach (`docs/reports/n2b-probe.md`).

**mpv attached in every single run below — ten out of ten, including all eight failures.** The check that asserts it, "mpv attached its own window inside the surface", never failed once. That is the defect N2b was filed for, and it is closed.

## What is not settled

Whether the picture reaches the screen, under Xvfb with llvmpipe.

| attempt | context     | measured                | runs | picture seen |
| ------- | ----------- | ----------------------- | ---- | ------------ |
| 1       | `x11egl`    | paused at 0:00          | 1    | 0            |
| 2       | `x11egl`    | paused at 0:00          | 3    | 1            |
| 3       | `x11egl`    | after starting playback | 3    | 0            |
| 4       | `x11` (GLX) | after starting playback | 3    | 1            |

Two passes out of ten. Starting playback did not help and may have hurt; GLX did not help either. The code is left on `x11egl`, the option with the better rationale, since neither is empirically better.

## The number that does not fit

The standalone probe — the `mpv` binary, same Xvfb, same `WAYLAND_DISPLAY`, `--wid` into a plain python-xlib window — read saturation **98.4** with `x11egl`, stable, first try. mpv renders into a foreign X11 window in this environment perfectly well when it is launched on its own.

So the difference is not mpv, not Xvfb, and not the Wayland variable. It is something about the same call made from inside the app, where the target is a GTK child window rather than a bare one.

## Two open hypotheses, neither tested

1. **A presentation defect in the embedding.** The surface is a GTK-created child window; mpv's EGL or GLX surface is created against it. Something about that pairing — the visual chosen for the GTK window, double buffering, when the window is mapped relative to when mpv builds its output — could leave frames presented somewhere the X screen grab never sees. If this is it, it is a product defect and it would show on real hardware too.
2. **An Xvfb and llvmpipe artefact.** Software rendering with no compositor, and a screen grab racing an unsynchronised swap. If this is it, there is nothing wrong with the product and the check simply cannot be run this way.

They predict opposite things, which is why the next step is not another run under Xvfb.

## The experiment Xvfb cannot do

**The owner opens the app in his own Wayland session, on real hardware, and looks at whether the video is there.** Build present at `target/debug/sublore` with the fix in it.

- Frame visible → hypothesis 2. N2b is done; the check gets restricted to the attachment, which is what it was filed for, and the pixel half is left to the X11 suite that already covers it.
- Frame missing, or black where the video should be → hypothesis 1. There is a real presentation defect in the embedding, and it is worth more than the rest of the queue, because it means the video does not work on the primary platform in the owner's own session.

One look decides it, and decides it better than any number of runs under a software rasteriser.

## For gate 2

- `x11egl` attaches reliably and presents unreliably under Xvfb software rendering; cause unknown, both hypotheses above still open.
- The standalone-versus-embedded gap is the sharpest clue and nobody has followed it.
- The check `e2e/scripts/wayland-attach-check.js` needs a real Wayland socket and refuses to run without one rather than passing on nothing. It is not in the CI job, which is headless X11.
