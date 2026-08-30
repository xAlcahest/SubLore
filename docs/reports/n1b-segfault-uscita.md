# The window sometimes segfaults on exit after the close gate saves — 2026-08-30

Found while running the N2b battery, not by looking for it. Filed as N1b: it belongs to the close gate (N1), not to the video work.

## What happens

`close-gate-check.js` fails at its eleventh check, "save exited the app with status 0", with:

```
exit was {"code":null,"signal":"SIGSEGV"}
```

Everything before it passes. The file is saved and the timestamped backup is kept, so **no user data is at risk in the observed failure**: the crash lands after the write, on the way out.

## How often

12 consecutive runs: 11 green, 1 SIGSEGV. A second occurrence the same afternoon, also on the save branch, makes 2 in roughly 17 runs. The discard branch goes through the same `close_window` and was not seen to crash; the shutdown check, which closes with no dialog at all, has never crashed.

## Where

`coredumpctl` caught both. The crashing thread is the main one:

```
#0  _gdk_x11_display_queue_events ()        libgdk-3
#1  gdk_display_get_event ()                libgdk-3
#2  gdk_event_source_dispatch ()            libgdk-3
#3  g_main_context_dispatch_unlocked ()     libglib-2.0
#6  gtk_main_iteration_do ()                libgtk-3
#7  gtk::auto::functions::main_iteration_do
#8  tao ... event_loop.rs:1154
#15 sublore_lib::run () at src-tauri/src/lib.rs:130
#16 sublore::main () at src-tauri/src/main.rs:41
```

GTK is still pulling X events out of a display that is being torn down. No Sublore frame appears above `run()`: the crash is inside the event loop, one iteration after we asked for the window to go.

## The lead

`ask_before_closing` answers off the main loop, and its callback calls `close_window`, which posts `window.destroy()` back to the main thread. The dialog's own GTK teardown is plausibly still in flight in that same iteration. Save differs from discard only in doing file I/O first, which moves the destroy a few milliseconds later relative to that teardown — which fits a race that only save has been seen to lose.

Not a proven cause. The next probe is to defer the destroy by one main-loop iteration and see whether the failure rate goes to zero over a run count large enough to mean something (30+, given 1 in 11).

## What this costs today

`close-gate-check.js` is in CI and will be red roughly once in eleven runs until this is fixed. The check is right and stays as it is: it is catching a real crash, and weakening it would only hide it.

## Correction, 2026-08-30, later the same day: the rfd thread is not the cause

The lead above was pursued and is **wrong**. The close gate's three message dialogs were moved off rfd and onto GTK dialogs created on the main thread, which removes rfd's second GTK thread from the process entirely. The crash survived.

A core dump from the new binary settles it. Only one thread in the process runs `gtk_main_iteration_do`, the main one; `grep` over every symbolised frame of every thread finds no `rfd` and no second GTK loop. The crashing frame is unchanged:

```
#0  _gdk_x11_display_queue_events   libgdk-3
#1  gdk_display_get_event           libgdk-3
#2  gdk_event_source_dispatch       libgdk-3
#6  gtk_main_iteration_do           libgtk-3
#8  tao ... event_loop
```

So the second GTK thread was real, was present in the earlier cores, and is bad practice on its own — but it is not what makes this crash. The claim in "The lead" above should be read as refuted.

What the 30-run battery established is in `n1b-trenta-corse.md`: 25 green, 1 SIGSEGV, 4 unexplained non-crash failures, and no evidence that the rate moved.

What is still true, and is now the whole of what is known: the main thread crashes inside GDK's X event queue, one loop iteration after `close_window` asked for `window.destroy()`, on the save branch, roughly once in a dozen closes under load.

Two candidate differences remain between this path and the ordinary close, which `shutdown-check.js` exercises constantly and which has never been seen to crash:

1. `close_window` calls `window.destroy()` instead of letting the close go through tao's own path.
2. A GTK dialog was created and destroyed moments earlier.

Neither is established. The discard branch does both and has not been seen to crash, which fits neither cleanly and is itself a fact worth explaining before the next attempt.
