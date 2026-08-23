#!/usr/bin/env python3
"""Send an ICCCM WM_DELETE_WINDOW ClientMessage to one window id.

Usage: close-window.py <window-id>

This is the app's real close path. `xdotool windowclose` is XDestroyWindow, which bypasses it
(and currently segfaults the app), and `xdotool windowquit` is a no-op without a window manager.
"""

import sys

from Xlib import X, display, protocol


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: close-window.py <window-id>", file=sys.stderr)
        return 2
    try:
        window_id = int(argv[1], 0)
    except ValueError:
        print(f"not a window id: {argv[1]!r}", file=sys.stderr)
        return 2

    dsp = display.Display()
    try:
        window = dsp.create_resource_object("window", window_id)
        event = protocol.event.ClientMessage(
            window=window,
            client_type=dsp.intern_atom("WM_PROTOCOLS"),
            data=(32, [dsp.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]),
        )
        window.send_event(event, event_mask=X.NoEventMask)
        dsp.flush()
        dsp.sync()
    finally:
        dsp.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
