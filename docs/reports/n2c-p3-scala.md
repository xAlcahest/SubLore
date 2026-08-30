# N2c / probe P3 — the two scale numbers, part one — 2026-08-30

Partial. Two of the three facts are established without running anything; the third needs the app on the owner's display and waits for the N1b measurement to free the machine.

## Measured, on the owner's session

Read from `:0` with `xrandr`, `xdpyinfo`, `xrdb` and the KDE output configuration. No process was started.

| fact                                            | value                                   |
| ----------------------------------------------- | --------------------------------------- |
| primary output                                  | DP-5, 3840x2160 at +0+0                 |
| second output                                   | DP-4, 2880x1620 at +3840+0              |
| X screen                                        | 6720x2160                               |
| `Xft.dpi`                                       | **144**                                 |
| `GDK_SCALE`, `GDK_DPI_SCALE`, `QT_SCALE_FACTOR` | all unset                               |
| KDE output scales                               | one output at **1.5**, every other at 1 |

144 is 1.5 x 96 exactly. Under rootless XWayland this is how KWin hands a fractional scale to X11 clients: the X coordinate space stays native — the 4K output really is 3840x2160 in X — and the 1.5 arrives as a font/UI DPI setting, not as a coordinate transform.

## Read from source: `window.scale_factor()` cannot carry 1.5

`tao-0.35.3/src/platform_impl/linux/window.rs:431` returns `self.scale_factor.load(..) as f64`, and the field at line 54 is an `Rc<AtomicI32>` seeded from GTK's own window scale factor. It is an integer by construction, so 1.5 is not a value it can hold. The monitor path (`monitor.rs:49`) is the same, `as f64` over an integer.

**Inference, not measurement:** with `GDK_SCALE` unset, GTK takes its integer scale factor from the XSETTINGS `Gdk/WindowScalingFactor`, which KWin leaves at 1 for fractional scales because it expresses them through `Xft.dpi` instead. So `window.scale_factor()` is expected to report **1** here. That expectation is what the remaining half of this probe checks; it is not yet a measured number.

The consequence for the code either way: `physical()` in `surface/mod.rs:51` multiplies by a factor that on this display is 1, and `logical()` — the path Linux actually uses (`surface/linux.rs:63-64`) — passes the page's numbers through untouched. Neither can produce a 1.5.

## Measured: `devicePixelRatio` is 1.5 and `scale_factor()` is 1.0

The app was launched on `:0` with the video fixture as a command-line argument — launch and arguments only, no synthetic input — with a temporary command that logged both numbers side by side. The instrumentation was removed afterwards and the binary rebuilt, with the build's exit status checked.

```
P3PROBE dpr=1.5  page=682x466  native=(scale_factor 1.0, inner_size 1024x700)
```

682 x 1.5 = 1023 and 466 x 1.5 = 699, against a native inner size of 1024x700. Within rounding, **one CSS pixel in this page is 1.5 X pixels**, and `window.scale_factor()` reports 1.0 — the integer the `tao` source said it would be. The second outcome of the two written down in advance is the one that happened.

A second, independent confirmation came from the window tree in the same run. The native surface sat at 512x120, which are the page's own numbers: at 1.5 it should have been 768x180. The rectangle reaching X is in CSS pixels.

One number in that run is not explained and is recorded rather than smoothed over: twelve seconds after launch `xwininfo` measured the toplevel at 800x600, while the startup log recorded an inner size of 1024x700. The window was resized after the log was written; nothing here measures by what.

## What this settles

The frontend reports `getBoundingClientRect()` in CSS pixels (`VideoStage.tsx:26-33`). The Linux path takes those numbers unchanged (`surface/linux.rs:63-64` uses `logical()`), so the surface is placed at 1/1.5 of its position and size. That is the whole of the misplacement.

The multiplier cannot come from the backend: `window.scale_factor()` is an integer and reports 1 on this display, so both `logical()` and `physical()` are wrong here — `physical()` would multiply by 1. It has to come from the page, which is the only party that knows the ratio.

That makes N2c's fix a change to what crosses the IPC boundary, not an arithmetic correction behind it, and CLAUDE.md section 6 applies: the region contract is a public interface, so the Windows path moves in the same change or it double-scales — there `scale_factor()` does carry the ratio, and `physical()` multiplies by it today.

## The fix, and the second mechanism the first attempt missed

Resolving the rectangle in the page was necessary and not sufficient. With the page sending native pixels and the Linux backend passing them through, `GDK_SCALE=2` put the surface at **four times** its rectangle rather than two.

There are two mechanisms, and only one of them is N2c:

| ratio comes from | GTK's own factor | does GDK re-apply it? | what the page must send | what the backend must do |
| ---------------- | ---------------- | --------------------- | ----------------------- | ------------------------ |
| page zoom, 1.5   | 1                | no                    | native pixels           | nothing                  |
| `GDK_SCALE`, 2   | 2                | yes                   | native pixels           | divide by 2              |

So the Linux backend now divides by the GDK window's own scale factor before handing the geometry to GDK, and Windows, which re-applies nothing, takes the numbers as they are. The factor still never crosses the IPC boundary: the page sends one number, and each platform undoes only what it is about to redo. The divisor is read from `gdk::Window::scale_factor()` where it lives, not threaded through from anywhere.

**A consequence worth stating plainly: `e2e/scripts/scaled-surface-check.js` does not prove N2c.** Under `GDK_SCALE` the old code was already correct, because GDK re-applied exactly the factor the page had not. That check guards the 4x regression this work nearly shipped, and nothing else. A fractional ratio cannot be produced on this harness at all — `Xft.dpi` through `xrdb` and a `gtk-xft-dpi` settings file both leave `devicePixelRatio` at 1, measured.

## Verified on the owner's display, 3840x2160 at 1.5

Launched with the fixture as a command-line argument, window 1024x700 native:

```
surface 592x180 at +432+500
```

The stage's CSS rectangle is 394.67x120 at 288,333, and 1.5 times that is 592x180 at 432,500 — exactly what X reports. Before the change the same launch put the surface at 395x120, the raw CSS numbers, over the transcription bar. A capture of the window shows the picture inside the stage.

The 800x600 suspect carried from the first half of this report **did not reproduce**: this launch kept the 1024x700 it asked for. It stays on the record as open rather than closed, because one clean launch is not an explanation.
