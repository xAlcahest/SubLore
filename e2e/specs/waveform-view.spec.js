/* global describe, it, before, document, window, getComputedStyle, WheelEvent */
/**
 * M2.4 W7, the half that moves the window: zoom and scroll, and the bounds on both.
 *
 * Everything here is read out of the canvas's own pixels, the way `waveform.spec.js` reads them and
 * for the same reason (decision 26). The fixture is the milestone's: six ten-second blocks
 * alternating a full-scale tone and digital silence, tone first, so a transition from tall ink to a
 * flat line is a block boundary and its column is a time.
 */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import { requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** The fixture's first boundary: tone until here, silence after. */
const FIRST_BOUNDARY_S = 10;
const DURATION_S = 60;

/** Deep enough that one pixel is one millisecond on any canvas this panel can be. */
const ZOOM_TO_DEEPEST = 12;

/** One wheel notch is 100 pixels, so 99 of them put 9900 ms at the left edge. */
const NOTCHES_TO_THE_BOUNDARY = 99;

/**
 * Every column where the drawing changes between tall and flat, as fractions of the width.
 *
 * Tall is a tone, flat is silence, and the threshold sits between the two by a wide margin: the
 * tone reaches at least 80% of the half-height and silence draws a line one pixel through it.
 */
function boundaries() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const context = canvas.getContext("2d");
    const panel = getComputedStyle(document.querySelector(".waveform")).backgroundColor;
    const background = (panel.match(/\d+/g) ?? ["0", "0", "0"]).map(Number);
    const middle = canvas.height / 2;
    const tall = [];
    for (let x = 0; x < canvas.width; x += 1) {
      const column = context.getImageData(x, 0, 1, canvas.height).data;
      let reach = 0;
      for (let y = 0; y < canvas.height; y += 1) {
        const i = y * 4;
        const differs =
          Math.abs(column[i] - background[0]) +
            Math.abs(column[i + 1] - background[1]) +
            Math.abs(column[i + 2] - background[2]) >
          24;
        if (differs) {
          reach = Math.max(reach, Math.abs(y + 0.5 - middle));
        }
      }
      tall.push(reach / middle > 0.8);
    }
    const changes = [];
    for (let x = 1; x < tall.length; x += 1) {
      if (tall[x] !== tall[x - 1]) {
        changes.push(x);
      }
    }
    return { changes, width: canvas.width };
  });
}

/** A compact reading of one row of the drawing, so "the window moved" is answerable at any zoom. */
function signature() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    const row = canvas.getContext("2d").getImageData(0, 0, canvas.width, 1).data;
    let out = "";
    for (let x = 0; x < canvas.width; x += 1) {
      out += row[x * 4] > 24 ? "1" : "0";
    }
    return out;
  });
}

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: rect.x * dpr, y: rect.y * dpr, width: rect.width * dpr, height: rect.height * dpr };
  }, selector);
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  clickAt(toplevel.absX + rect.x + rect.width / 2, toplevel.absY + rect.y + rect.height / 2);
}

/** A wheel notch over the canvas, at a column of it, with or without ctrl. */
async function wheelAt(column, notches, ctrl) {
  await browser.execute(
    (at, count, withCtrl) => {
      const canvas = document.querySelector(".waveform__canvas");
      const box = canvas.getBoundingClientRect();
      const clientX = box.x + at / window.devicePixelRatio;
      for (let step = 0; step < Math.abs(count); step += 1) {
        canvas.dispatchEvent(
          new WheelEvent("wheel", {
            bubbles: true,
            cancelable: true,
            clientX,
            clientY: box.y + box.height / 2,
            deltaY: count > 0 ? 100 : -100,
            ctrlKey: withCtrl,
          }),
        );
      }
    },
    column,
    notches,
    ctrl,
  );
  await browser.pause(120);
}

describe("the waveform's window on the media", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__open-video") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
    await clickElement(toplevel, ".toolbar__open-video");
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, requireWaveformFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(
      async () => {
        const read = await boundaries();
        return read !== null && read.changes.length >= 5 ? read : null;
      },
      { timeout: 30000, message: "the whole file to be drawn, with its five block boundaries" },
    );
  });

  it("opens on the whole file, with every block boundary on screen", async () => {
    const read = await boundaries();
    expect(read.changes).toHaveLength(5);
    // Ten seconds of sixty is a sixth of the width, and each boundary is a further sixth along.
    for (let index = 0; index < read.changes.length; index += 1) {
      const seconds = (read.changes[index] / read.width) * DURATION_S;
      expect(Math.abs(seconds - FIRST_BOUNDARY_S * (index + 1))).toBeLessThan(0.5);
    }
  });

  it("puts the block boundary at the millisecond the media says, at the deepest zoom", async () => {
    await wheelAt(0, -ZOOM_TO_DEEPEST, true);
    // Hard against the left edge first, so the window starts at zero exactly and owes nothing to
    // where the pointer was when the zoom happened. A wheel notch is 100 pixels, and at the deepest
    // zoom a pixel is a millisecond, so the arithmetic from here is exact.
    await wheelAt(0, -Math.ceil(DURATION_S * 10), false);
    await wheelAt(0, NOTCHES_TO_THE_BOUNDARY, false);

    const read = await boundaries();
    // 9900 ms at the left edge and the boundary at 10000 puts it at column 100. A column either
    // side is a millisecond either side, which is W7's criterion.
    expect(read.changes).toHaveLength(1);
    expect(
      Math.abs(read.changes[0] - (FIRST_BOUNDARY_S * 1000 - NOTCHES_TO_THE_BOUNDARY * 100)),
    ).toBeLessThanOrEqual(1);
  });

  it("shows a second or so at the deepest zoom, where sixty seconds held five boundaries", async () => {
    const read = await boundaries();
    // One boundary at most: the blocks are ten seconds apart and the window is about a second.
    expect(read.changes.length).toBeLessThanOrEqual(1);
  });

  it("does not zoom past the deepest, and comes back out to the whole file", async () => {
    const deepest = await boundaries();
    await wheelAt(deepest.width / 2, -ZOOM_TO_DEEPEST, true);
    expect((await boundaries()).changes).toEqual(deepest.changes);

    await wheelAt(deepest.width / 2, ZOOM_TO_DEEPEST * 2, true);
    const out = await boundaries();
    expect(out.changes).toHaveLength(5);
  });

  it("scrolls with the arrow keys and stops at both ends of the media", async () => {
    await wheelAt(0, -ZOOM_TO_DEEPEST, true);
    await browser.execute(() => document.querySelector(".waveform__canvas").focus());

    // Read as a row of the drawing rather than as boundaries: at this zoom the window can hold no
    // block boundary at all, and then "did it move" has no answer in boundaries.
    const atStart = await signature();
    pressKey("Left");
    pressKey("Left");
    await browser.pause(150);
    expect(await signature()).toBe(atStart);

    // And the window does move when there is somewhere to go.
    pressKey("Right");
    await browser.pause(150);
    expect(await signature()).not.toBe(atStart);
  });
});
