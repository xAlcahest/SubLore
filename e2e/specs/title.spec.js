/* global describe, it */
import { browser, expect } from "@wdio/globals";

import { waitFor } from "../lib/proc.js";
import { windowHeight, windowTitle, windowWidth } from "../lib/paths.js";
import { findWindowsWithAppGeometry, rootTree } from "../lib/x11.js";

describe("app launch", () => {
  it("native window title is Sublore", async () => {
    // Select by geometry, then assert the name: a wrong title must fail on the name, not on
    // "no window found". GTK's 10x10 group-leader answers to the same name and must never match.
    const windows = await waitFor(
      () => {
        const found = findWindowsWithAppGeometry();
        return found.length > 0 ? found : null;
      },
      { timeout: 30000, message: `a ${windowWidth}x${windowHeight} toplevel to appear` },
    );

    expect(windows).toHaveLength(1);
    const actual = windows[0].name;
    if (actual !== windowTitle) {
      throw new Error(
        `native window title is ${JSON.stringify(actual)}, expected ${JSON.stringify(windowTitle)}.` +
          `\n${rootTree()}`,
      );
    }
  });

  it("document title is Sublore", async () => {
    expect(await browser.getTitle()).toBe(windowTitle);
  });
});
