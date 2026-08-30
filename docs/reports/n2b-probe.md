# N2b probe — which mpv option attaches to an X11 `wid` inside a Wayland session?

Run 2026-08-30 on Linux, Fedora, real Wayland socket present (`/run/user/1000/wayland-0`), mpv 0.41.0, Xvfb 1024x700 as the X display. Probe kept outside the repo (`scratchpad/demo/n2b-probe.sh`).

## The question

With `WAYLAND_DISPLAY` set, libmpv does not draw into the X11 window it is handed. `main.rs` already forces `GDK_BACKEND=x11` before GTK picks a backend, so GTK is not the one choosing; the app's own comment there says `--wid` needs an X11 window id and the Wayland VO has no equivalent. But `player.rs` sets `vo` and `ao` only in headless mode: the embedded path pins nothing, so mpv's `gpu-context=auto` sees a Wayland display and takes it, and the frames go to the real compositor instead of into our window.

Asked of libmpv directly through the `mpv` binary, against a plain X11 container window created with python-xlib, so each candidate could be tried without rebuilding the app. Attachment is judged the way the harness judges it: mpv's own child window inside the container, **and** saturated pixels on screen.

## Results

`WAYLAND_DISPLAY=wayland-0` and `DISPLAY=:96` set for every run.

| option                         | mpv child windows | saturation | attached |
| ------------------------------ | ----------------- | ---------- | -------- |
| none (today's behaviour)       | 0                 | 0          | **no**   |
| `--gpu-context=x11egl`         | 1                 | 98.4       | yes      |
| `--gpu-context=x11` (GLX)      | 1                 | 106.9      | yes      |
| `--gpu-context=x11vk` (Vulkan) | 1                 | 107.8      | yes      |
| `--vo=x11` (software)          | 1                 | 107.5      | yes      |
| `--vo=xv`                      | 0                 | 0          | no       |

The first row is the defect, reproduced deliberately rather than inferred. `--vo=xv` failing is expected: Xvfb offers no Xv adaptor, and it is not a candidate anyway.

## The choice: `gpu-context=x11egl`

It pins the platform and nothing else. `vo=gpu` stays, so hardware acceleration stays; only the context selection stops being `auto`, which is the part that was picking Wayland.

The three rejected alternatives, with the reason:

- `vo=x11` is the software output. It attaches, and it throws away acceleration to do it, which would put the §7 budgets at risk on exactly the machines that need them most.
- `x11vk` forces Vulkan. CLAUDE.md §2 wants Vulkan where available with a CPU fallback always working, never as a hard requirement, and pinning it here makes it one.
- `x11` is GLX, the older path. EGL is what mpv's own defaults prefer on X11 today, and choosing the legacy one needs a reason this probe did not find.

If EGL is ever missing, mpv fails to build its output and `video_open` returns an error the app already surfaces, rather than failing silently. That is the acceptable direction for this failure.

## Scope, and what this does not settle

Linux only. The Windows path never had this problem — there is no second display server to be picked — and the option is set behind a `cfg` for that reason.

Not measured: whether the frame rate or GPU decode differ between the current `auto` (Wayland, off-window) and pinned x11egl, because the current behaviour puts nothing in our window at all, so there is no before to compare against. The §7 budgets are measured elsewhere and unchanged by this.
