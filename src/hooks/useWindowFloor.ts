import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useState } from "react";

import { widestRow } from "../measure";

/**
 * The rows the shell cannot draw narrower than what is in them, so the window cannot be narrower
 * than the widest of them (S1).
 *
 * `.menubar` and `.toolbar` are one line of controls each, every one of them drawn whether or not
 * it can run and none of them shrinking or wrapping (CLAUDE.md, owner ruling 2026-09-03), and
 * `.cuelist__head` is the grid's column skeleton. Its timing and counter cells are `flex: 0 0`
 * widths and ask for those; Text and, on an ASS, Style and Actor take the row's slack and declare
 * no width, so `widestRow` counts them at zero and what an open document adds to this reading is
 * two column gaps. That is the mechanism behind the rule that opening a file may not resize the
 * user's window (grid-columns-tasks.md G3). The rows that are not here are the rows that wrap: the
 * status bar, the find band and the transcription panel all set `flex-wrap: wrap` and give up a
 * line rather than a pixel of width.
 */
const UNSHRINKABLE_ROWS = [".menubar", ".toolbar", ".cuelist__head"];

/**
 * The narrowest the window may be, measured off the shell rather than written down, and carried to
 * the window every time it changes.
 *
 * Every width in those rows is a width the machine's fonts decide: the same toolbar is about a
 * tenth wider under DejaVu Sans than under the families this interface was drawn against, which is
 * the difference between fitting in 1024 pixels at 150 per cent and hanging past the edge. A number
 * in this repo could only have been right on one machine.
 *
 * @param scale the interface size, which every width in those rows is drawn against.
 * @param chrome changes when the bars' contents change, which is what a module contributing a menu
 *   title does. Their labels are otherwise the same at every window size and in every state.
 * @param block what the row of panels under the chrome asks for, or null while the video panel's
 *   own floor has not been read yet: a window told about a floor short of a panel has no floor.
 */
export function useWindowFloor(
  scale: number,
  chrome: string,
  block: number | null,
): { width: number | null; failed: boolean } {
  const [rows, setRows] = useState<number | null>(null);
  const [refused, setRefused] = useState(false);
  const [missing, setMissing] = useState(false);

  // Before the paint, for the same reason the transport's floor is read there: the number the
  // window is given must not be one read against type that has already been replaced. Nothing here
  // depends on how wide the window is, so a drag on the window's edge costs no measurement.
  useLayoutEffect(() => {
    let live = true;
    const measure = () => {
      let widest = 0;
      let read = 0;
      for (const selector of UNSHRINKABLE_ROWS) {
        const row = document.querySelector<HTMLElement>(selector);
        const asked = row === null ? null : widestRow(row, []);
        if (asked !== null) {
          widest = Math.max(widest, asked);
          read += 1;
        }
      }
      if (!live) {
        return;
      }
      // A row that is not in the page is one this list names under a name the markup no longer
      // uses, and a floor short of a row is a bar cut off rather than a window that refused.
      setMissing(read < UNSHRINKABLE_ROWS.length);
      setRows(widest);
    };
    measure();
    // Type can arrive after the first paint, and every width above is the width of some type. The
    // promise has no failure: a page whose fonts never settle keeps the reading taken above.
    void document.fonts.ready.then(measure, () => {});
    return () => {
      live = false;
    };
  }, [scale, chrome]);

  // Up to the whole pixel: the readings are in fractions of one and a window size is not.
  const width = rows === null || block === null ? null : Math.ceil(Math.max(rows, block));

  useEffect(() => {
    if (width === null) {
      return;
    }
    const carry = () => {
      void invoke("layout_set_minimum_width", { width })
        .then(() => setRefused(false))
        .catch(() => setRefused(true));
    };
    carry();
    // A smallest size is a hint to whatever places the window, and not everything that places a
    // window honours one. Asked again only while the window is under the floor, so a resize that
    // respects it costs nothing and one that does not is put back once.
    const putBack = () => {
      if (window.innerWidth < width) {
        carry();
      }
    };
    window.addEventListener("resize", putBack);
    return () => window.removeEventListener("resize", putBack);
  }, [width]);

  return { width, failed: refused || missing };
}
