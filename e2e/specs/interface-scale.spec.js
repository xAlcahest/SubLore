/* global describe, it, before, after, document, window */
/**
 * S1 and S2: the interface has a size the user picks, and the panel floors move with it.
 *
 * Written in the shape of `dividers.spec.js`, which owns two of the three edges dragged here: the
 * gestures are real presses, travels and releases through X11, because a synthetic pointer event
 * exercises React and proves nothing about whether a hand can place an edge.
 *
 * Nothing here reads the stored number or the custom property that carries it. A shell that stored
 * the size and never redrew would satisfy every one of those readings, so the size is asserted as
 * the thing the complaint was about: how tall the type and the controls come out. The video edge is
 * asserted at its floor while the button is still down, for the reason `pressAndTravel` exists.
 */
import { rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clippedAtWindowEdge } from "../lib/clipping.js";
import {
  clickAt,
  dragAt,
  focusWindow,
  pressAndTravel,
  releaseButton,
  resizeWindow,
} from "../lib/input.js";
import { repoRoot, requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { interfaceScale } from "../lib/scale.js";
import { findToplevel, rootTree } from "../lib/x11.js";

const VIDEO_SASH = ".sash--video";
const GRID_SASH = ".sash--grid";
const WAVEFORM_SASH = ".sash--waveform";

const SUBTITLE = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");

/**
 * The bounds `dividers.spec.js` and `waveform-sash.spec.js` mirror from `src/App.tsx`, each the
 * number at 100 per cent and each taken against the interface size before it is used.
 */
const MIN_TOOLS_WIDTH = 176;
const MIN_CURRENT_LINE = 72;
const MIN_WAVEFORM_HEIGHT = 64;

/** The size a launch with nothing stored opens at (S1). */
const DEFAULT_PERCENT = 110;

/** The three sizes S2 states its criterion at, all of them on the View menu. */
const FLOOR_PERCENTS = [90, 110, 150];

/** Percentage widths and scaled type both land on fractions of a pixel. */
const SLOP_PX = 1;

/** The second window size S1 states its criterion at. The run's screen is sized to hold it. */
const WIDE_WIDTH = 1920;
const WIDE_HEIGHT = 1080;

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return {
      cssWidth: rect.width,
      cssHeight: rect.height,
      midX: (rect.x + rect.width / 2) * dpr,
      midY: (rect.y + rect.height / 2) * dpr,
    };
  }, selector);
}

/**
 * How big the interface is drawn, in the terms the complaint was written in: the type on a label,
 * on a control and on a grid row, with the box each of them sits in.
 */
async function drawnSizes() {
  const drawn = await readDrawnSizes();
  const missing = Object.entries(drawn)
    .filter(([, part]) => part === null)
    .map(([name]) => name);
  if (missing.length > 0) {
    throw new Error(`these parts of the interface are missing from the DOM: ${missing.join(", ")}`);
  }
  return drawn;
}

function readDrawnSizes() {
  return browser.execute(() => {
    const read = (css) => {
      const element = document.querySelector(css);
      if (element === null) {
        return null;
      }
      return {
        type: Number.parseFloat(window.getComputedStyle(element).fontSize),
        height: element.getBoundingClientRect().height,
      };
    };
    return {
      menuTitle: read(".menubar__title--view"),
      transportButton: read(".controls__button"),
      timesLabel: read(".currentline__label"),
      gridHeader: read(".cuelist__head"),
      gridRow: read(".cuelist__row"),
    };
  });
}

/** Every number the three edges trade, read in one round trip so they describe one layout. */
function shellSizes() {
  return browser.execute(() => {
    const width = (css) => document.querySelector(css)?.getBoundingClientRect().width ?? null;
    const height = (css) => document.querySelector(css)?.getBoundingClientRect().height ?? null;
    return {
      video: width(".shell__video"),
      tools: width(".shell__tools"),
      top: width(".shell__top"),
      block: height(".shell__body"),
      grid: height(".shell__grid"),
      line: height(".currentline"),
      waveform: height(".waveform"),
      transport: height(".controls"),
    };
  });
}

/**
 * The grid's header and the rows wholly inside its scrolling viewport: what "showing its header and
 * three rows" is, counted rather than inferred from a height.
 */
function readGridShows() {
  return browser.execute((slop) => {
    const header = document.querySelector(".cuelist__head");
    const list = document.querySelector(".cuelist");
    if (header === null || list === null) {
      return null;
    }
    const viewport = list.getBoundingClientRect();
    const whole = Array.from(document.querySelectorAll(".cuelist__row")).filter((row) => {
      const rect = row.getBoundingClientRect();
      return rect.top >= viewport.top - slop && rect.bottom <= viewport.bottom + slop;
    });
    return { header: header.getBoundingClientRect().height, rows: whole.length };
  }, SLOP_PX);
}

async function expectGridShows(rows) {
  const shown = await readGridShows();
  if (shown === null) {
    throw new Error("the cue grid's header or its list is missing from the DOM");
  }
  expect({ header: shown.header > 0, rows: shown.rows }).toEqual({ header: true, rows });
}

/**
 * No window manager under Xvfb, so the toplevel origin is also the viewport origin.
 *
 * The destination is kept inside the window: `xdotool` refuses a pointer off the screen, and a drag
 * that asks for one fails as an error rather than as an edge that stopped where it was told to.
 */
async function dragSash(toplevel, selector, dx, dy) {
  const sash = await rectOf(selector);
  if (sash === null) {
    throw new Error(`${selector} is missing from the DOM, so there is nothing to drag`);
  }
  const inside = (value, span) => Math.min(Math.max(value, 1), span - 2);
  dragAt(
    toplevel.absX + sash.midX,
    toplevel.absY + sash.midY,
    toplevel.absX + inside(sash.midX + dx, toplevel.width),
    toplevel.absY + inside(sash.midY + dy, toplevel.height),
  );
  // The release is what stores the size, so this reads a settled layout, never a mid-drag one.
  await browser.pause(250);
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  if (rect === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  clickAt(toplevel.absX + rect.midX, toplevel.absY + rect.midY);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Pick one of the View menu's five sizes, through the menu, the way a person reaches it. */
async function pickSize(toplevel, percent) {
  const item = `.menubar__item--view-interface-scale-${percent}`;
  await clickElement(toplevel, ".menubar__title--view");
  await waitFor(() => present(item), {
    timeout: 5000,
    message: `the View menu to offer ${percent} per cent`,
  });
  await clickElement(toplevel, item);
  await waitFor(
    async () => (Math.abs((await interfaceScale()) - percent / 100) < 0.001 ? 1 : null),
    { timeout: 5000, message: `the interface to be drawn at ${percent} per cent` },
  );
  // Picking is also what stores the size, and the checks below relaunch the app: a pause here is
  // the same one `dragSash` takes, for the same write.
  await browser.pause(250);
}

/** Resize the app window and wait until the page has been laid out at the new width. */
async function resizeTo(id, width, height) {
  resizeWindow(id, width, height);
  await waitFor(
    async () => ((await browser.execute(() => window.innerWidth)) === width ? 1 : null),
    { timeout: 15000, message: `the page to be laid out ${width} CSS pixels wide` },
  );
  const toplevel = findToplevel({ width, height });
  if (toplevel === null) {
    throw new Error(`no ${width}x${height} "Sublore" toplevel after the resize.\n${rootTree()}`);
  }
  return toplevel;
}

async function attachToApp() {
  const toplevel = await waitFor(findToplevel, {
    timeout: 30000,
    message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
  });
  focusWindow(toplevel.id);
  await waitFor(
    () => browser.execute(() => document.querySelector(".toolbar__video-open") !== null),
    { timeout: 30000, message: "the app UI to render" },
  );
  return toplevel;
}

/** A document in the grid and a video on the stage: all three edges sit between panels holding both. */
async function openTheFixtures(toplevel) {
  await clickElement(toplevel, ".toolbar__file-open-subtitle");
  const subtitleChooser = await waitForChooser("Choose a subtitle");
  await answerChooser(subtitleChooser, SUBTITLE, "subtitle");
  focusWindow(toplevel.id);
  await waitFor(() => present(".cuelist__row"), {
    timeout: 20000,
    message: "the cue grid to fill",
  });

  await clickElement(toplevel, ".toolbar__video-open");
  const videoChooser = await waitForChooser("Choose a video");
  await answerChooser(videoChooser, requireWaveformFixture(), "video");
  focusWindow(toplevel.id);
  await waitFor(() => present(".waveform"), {
    timeout: 40000,
    message: "the waveform panel to appear",
  });
}

const storedLayout = () =>
  path.join(process.env.SUBLORE_E2E_DATA_HOME, "com.sublore.app", "layout.json");

describe("the interface size", () => {
  let toplevel = null;

  // The store is shared with every other spec in the run, and the first check below reads the size
  // a launch with nothing stored opens at, so this one starts from no file rather than from
  // whatever the spec before it left.
  before(async () => {
    requireWaveformFixture();
    rmSync(storedLayout(), { force: true });
    await browser.reloadSession();
    toplevel = await attachToApp();
    await openTheFixtures(toplevel);
  });

  after(() => {
    rmSync(storedLayout(), { force: true });
  });

  it("opens at 110 per cent, which is a tenth larger than the size the interface used to be", async () => {
    const opened = await drawnSizes();

    await pickSize(toplevel, 100);

    // Against the size the menu itself offers, not against a number: 110 per cent means this and
    // nothing else, and the ratio holds whatever the browser's own root size turns out to be.
    const at100 = await drawnSizes();
    for (const [part, drawn] of Object.entries(opened)) {
      expect({ part, ratio: Number((drawn.type / at100[part].type).toFixed(3)) }).toEqual({
        part,
        ratio: DEFAULT_PERCENT / 100,
      });
    }
    expect(opened.menuTitle.height).toBeGreaterThan(at100.menuTitle.height);
  });

  it("keeps the whole interface inside the window at both ends of the range, at both window sizes", async () => {
    for (const percent of [90, 150]) {
      await pickSize(toplevel, percent);
      for (const size of [
        { width: windowWidth, height: windowHeight },
        { width: WIDE_WIDTH, height: WIDE_HEIGHT },
      ]) {
        toplevel = await resizeTo(toplevel.id, size.width, size.height);
        const at = `at ${percent} per cent in ${size.width}x${size.height}`;
        expect({ at, clipped: await clippedAtWindowEdge(SLOP_PX) }).toEqual({ at, clipped: [] });
        const across = await browser.execute(() => ({
          document: document.documentElement.scrollWidth,
          body: document.body.scrollWidth,
          client: document.documentElement.clientWidth,
        }));
        const sideways = across.document > across.client || across.body > across.client;
        expect({ at, sideways }).toEqual({ at, sideways: false });
      }
      toplevel = await resizeTo(toplevel.id, windowWidth, windowHeight);
    }
  });

  it("draws the type half again as tall at 150 per cent, and the controls with it", async () => {
    await pickSize(toplevel, 100);
    const at100 = await drawnSizes();

    await pickSize(toplevel, 150);

    const at150 = await drawnSizes();
    for (const [part, drawn] of Object.entries(at150)) {
      expect({ part, ratio: Number((drawn.type / at100[part].type).toFixed(3)) }).toEqual({
        part,
        ratio: 1.5,
      });
    }
    // The boxes follow the type but not to the last pixel: each carries a one-pixel border that is
    // one pixel at every size, so a control that grew by half loses a little of it to the border.
    for (const part of ["menuTitle", "transportButton", "gridHeader"]) {
      const grew = at150[part].height / at100[part].height;
      expect({ part, follows: grew > 1.4 && grew <= 1.5 }).toEqual({ part, follows: true });
    }
    // The one part that does not follow, said here rather than left for a reader to notice: a cue
    // row's box is the fixed 28px `ROW_HEIGHT` in CueList.tsx at every size, so its type grows and
    // its box does not. S1 asks for the row as well as the type, and this is the half it gets.
    expect(at150.gridRow.height).toBe(at100.gridRow.height);
  });

  it("leaves the three panels at the proportions the sashes were left at when the size changes", async () => {
    await pickSize(toplevel, 100);
    await dragSash(toplevel, VIDEO_SASH, 120, 0);
    await dragSash(toplevel, GRID_SASH, 0, 40);
    await dragSash(toplevel, WAVEFORM_SASH, 0, 30);
    const left = await shellSizes();

    await pickSize(toplevel, 150);

    const bigger = await shellSizes();
    // The video is stored as a share of a row that is itself narrower at 150, because the rail beside
    // it is in rem too, so the share is what has to survive and not the width.
    expect(bigger.video / bigger.top).toBeCloseTo(left.video / left.top, 2);
    expect(bigger.video).toBeLessThan(left.video);
    // The waveform's own measurements stay in device pixels: a peak bucket is one millisecond and a
    // fraction of a pixel has nothing to draw.
    expect(bigger.waveform).toBe(left.waveform);
    expect(bigger.block).toBe(left.block);
  });

  it("holds the video edge at its floor with the transport on one row, at 150 per cent, while the pointer is still travelling", async () => {
    await pickSize(toplevel, 150);
    // Room to travel first: at 150 the panel opens within a pixel of its own floor, so a drag from
    // there would prove the floor by not moving at all.
    await dragSash(toplevel, VIDEO_SASH, 2000, 0);
    const wide = await shellSizes();

    await dragSash(toplevel, VIDEO_SASH, -2000, 0);

    const settled = await shellSizes();
    expect(settled.video).toBeLessThan(wide.video);
    // The claim the floor was measured for: the transport is the height it is when it has room, so
    // it is on the row it was on, not wrapped onto four and eating the picture.
    expect(settled.transport).toBe(wide.transport);

    // The same reading taken during the gesture. Everything above is equally true of a panel that
    // ignores the pointer and jumps once it is let go, which is the mutation that left every
    // divider assertion green.
    await dragSash(toplevel, VIDEO_SASH, 2000, 0);
    const sash = await rectOf(VIDEO_SASH);
    if (sash === null) {
      throw new Error(`${VIDEO_SASH} is missing from the DOM, so there is nothing to drag`);
    }
    const fromX = toplevel.absX + sash.midX;
    const fromY = toplevel.absY + sash.midY;
    try {
      // To the far side of the window: the width the pointer is asking for is past zero, so a panel
      // still following it is not at a floor.
      pressAndTravel(fromX, fromY, toplevel.absX + 1, fromY);
      const held = await waitFor(
        async () => {
          const now = await shellSizes();
          return now.video <= settled.video + SLOP_PX ? now : null;
        },
        {
          timeout: 5000,
          message:
            `the video panel to follow the pointer from ${Math.round(wide.video)} down to ` +
            `${Math.round(settled.video)} while the button is still down`,
        },
      );
      // It stopped there rather than carrying on: the pointer is asking for a width past zero.
      expect(held.video).toBeGreaterThanOrEqual(settled.video - SLOP_PX);
      expect(held.transport).toBe(wide.transport);
    } finally {
      // Never leave the button down: it lands on whatever the next check clicks.
      releaseButton();
    }
    await browser.pause(250);
  });

  for (const percent of FLOOR_PERCENTS) {
    it(`leaves both panels usable at each edge's floor, at ${percent} per cent`, async () => {
      const scale = percent / 100;
      await pickSize(toplevel, percent);
      // The block is opened up first, so each edge has a range to be driven across: the stored
      // default block height is a pixel count that does not move with the interface, so at 150 it
      // opens already at its own floor and no drag of the grid edge would happen at all.
      await dragSash(toplevel, GRID_SASH, 0, 2000);

      // The video edge, both ends. The transport's unwrapped height is measured at the ceiling and
      // the floor is asked to match it, rather than a number standing in for one row.
      await dragSash(toplevel, VIDEO_SASH, 2000, 0);
      const atCeiling = await shellSizes();
      expect(atCeiling.tools).toBeGreaterThanOrEqual(MIN_TOOLS_WIDTH * scale - SLOP_PX);

      await dragSash(toplevel, VIDEO_SASH, -2000, 0);
      const atVideoFloor = await shellSizes();
      expect(atVideoFloor.video).toBeLessThan(atCeiling.video);
      expect(atVideoFloor.transport).toBe(atCeiling.transport);
      expect(atVideoFloor.line).toBeGreaterThanOrEqual(MIN_CURRENT_LINE * scale - SLOP_PX);

      // The grid edge, both ends: the current line at one, the header and three rows at the other.
      await dragSash(toplevel, GRID_SASH, 0, -2000);
      const atGridFloor = await shellSizes();
      expect(atGridFloor.block).toBeLessThan(atCeiling.block);
      expect(atGridFloor.line).toBeGreaterThanOrEqual(MIN_CURRENT_LINE * scale - SLOP_PX);
      expect(atGridFloor.transport).toBe(atCeiling.transport);

      await dragSash(toplevel, GRID_SASH, 0, 2000);
      await expectGridShows(3);

      // The waveform edge's floor, the one the criterion names that `dividers.spec.js` does not own.
      await dragSash(toplevel, WAVEFORM_SASH, 0, -2000);
      const atWaveFloor = await shellSizes();
      expect(Math.round(atWaveFloor.waveform)).toBe(Math.round(MIN_WAVEFORM_HEIGHT * scale));
      expect(atWaveFloor.line).toBeGreaterThanOrEqual(MIN_CURRENT_LINE * scale - SLOP_PX);
    });
  }

  it("opens at the size that was picked", async () => {
    await pickSize(toplevel, 125);
    const picked = await drawnSizes();

    await browser.execute(() => {
      window.name = "before-the-relaunch";
    });
    await browser.reloadSession();
    toplevel = await attachToApp();
    expect(await browser.execute(() => window.name)).not.toBe("before-the-relaunch");
    await openTheFixtures(toplevel);

    const reopened = await drawnSizes();
    expect(reopened).toEqual(picked);
  });

  it("opens at 110 per cent again once the stored layout is gone", async () => {
    // The other half of the claim above: a deleted store is a first launch, and 125 was picked by
    // hand, so a size that survives this is one nothing is reading.
    const picked = await drawnSizes();
    rmSync(storedLayout(), { force: true });

    await browser.reloadSession();
    toplevel = await attachToApp();
    await openTheFixtures(toplevel);
    const opened = await drawnSizes();
    expect(opened).not.toEqual(picked);

    await pickSize(toplevel, 100);
    const at100 = await drawnSizes();
    expect(Number((opened.gridRow.type / at100.gridRow.type).toFixed(3))).toBe(
      DEFAULT_PERCENT / 100,
    );
  });
});
