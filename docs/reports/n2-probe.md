# N2 probe — can the video surface be hidden and shown again?

Run 2026-08-29 on Linux, X11 under Xvfb 1024x700, debug build at `6d6d3ab`. Probe script kept outside the repo (`scratchpad/demo/n2-probe.mjs`), per the practice of not committing throwaway harnesses.

## The question

N2's acceptance criterion asserts on the visible frame, not on a state flag, because `show()` is called in exactly one place — inside `video_open` (`src-tauri/src/video/mod.rs:106`) — and its own comment warns it must run before mpv builds its video output, since mpv leaves its output unmapped if the surface it was given is unmapped (`surface/mod.rs:82-84`). Hide-then-show on an already-open video had never been exercised.

Asked of mpv directly, at the X level: `hide()` is `window.hide()` and `show()` is `window.show()` plus raise (`surface/linux.rs:70-78`), so the probe unmaps and maps the surface window with `xdotool` and looks at the pixels. Measuring the pixel spread over the surface rectangle: a colour-bar frame scores ~0.38, an empty hole scores 0.

Two paths, because they are different inside mpv, and the paused one is the one a user meets: pause, open a dialog, close it.

## Case A — video playing

| moment       | map state  | spread |
| ------------ | ---------- | ------ |
| before hide  | IsViewable | 0.3833 |
| while hidden | IsUnMapped | 0.0000 |
| after show   | IsViewable | 0.3832 |
| 1.5 s later  | IsViewable | 0.3850 |

**Picture comes back, and it is advancing** (the spread keeps moving, so frames are still being drawn).

## Case B — video paused

| moment       | map state  | spread |
| ------------ | ---------- | ------ |
| before hide  | IsViewable | 0.3848 |
| while hidden | IsUnMapped | —      |
| after show   | IsViewable | 0.3848 |

**Picture comes back with no nudge.** No seek, no play, no forced redraw: the probe issues nothing but the map, waits three seconds and looks. Confirmed visually as well as numerically: the screenshot shows the colour bars back with the transport still reading Play / 0:08, so playback was not restarted to get them.

## Consequence for N2

mpv restores its output on remap in both states, so N2 is what its criterion says it is: build the re-show path, assert on the frame. No design change, no owner decision needed.

One correction to a comment in the code, found on the way: `surface/mod.rs:82-84` and `e2e/specs/video.spec.js:114` both say mpv creates a window inside our surface. It does, but only once mpv has really attached — in the runs where it had not, the surface had **zero** children while still reporting `IsViewable`. The child window is therefore the honest signal that mpv is attached; the map state alone is not.

## What the probe got wrong first, and why it matters for the test

Three runs. The first two produced numbers that looked like findings and were not, and the reason is worth writing down because the N2 test will meet it.

The probe spawned the app inheriting the shell's environment, which carries `WAYLAND_DISPLAY`. GTK then prefers Wayland even under Xvfb, mpv never attaches to the X11 surface it was handed, and the stage keeps showing the "No video open." placeholder while the transport happily reports `0:02 / 1:00`. Every measurement of that surface was measuring the webview underneath. The first run's verdict — "the picture never comes back" — was an artifact.

What caught it was looking at the screenshot instead of trusting the number, and then a precondition that refuses to measure a case whose starting point is already wrong. The N2 test carries the same precondition for the same reason.

**Fragility worth recording, not a live defect:** `e2e/wdio.conf.js` pins `XDG_DATA_HOME` and the ASR paths but does not pin `GDK_BACKEND` or clear `WAYLAND_DISPLAY`. The suite passes today, and the video spec would not pass vacuously if this bit — it waits for mpv's child window, which is exactly what goes missing — so the assertion is sound. But the harness works by luck of the environment rather than by construction, and a CI runner or a developer machine with a Wayland session in the environment would turn the video specs red for a reason that has nothing to do with the code.
