/* global describe, it, before, after, document, window */
/**
 * D1: the video's right edge and the top block's bottom edge are draggable, both are remembered,
 * and both move the native video surface with them.
 *
 * The drags are real presses, moves and releases through X11, the way `waveform-sash.spec.js`
 * drives the edge it owns: a synthetic pointer event would exercise React and prove nothing about
 * whether a hand can grab a four-pixel strip.
 *
 * The surface is read through the X11 window tree and never off the picture, which is what
 * `../lib/surface.js` exists for. A drag is a resize, so every drag here is also an M0.2 check.
 */
import { rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, dragAt, focusWindow, pressAndTravel, releaseButton } from "../lib/input.js";
import { repoRoot, requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { waitForSurfaceOnStage } from "../lib/surface.js";
import { findToplevel } from "../lib/x11.js";

const VIDEO_SASH = ".sash--video";
const GRID_SASH = ".sash--grid";

const SUBTITLE = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");

/**
 * Mirrors the bounds `src/App.tsx` measured off the rendered shell at this window size, each of
 * them the number at 100 per cent. This file runs at the size the app opens at, which is larger, so
 * each is read here as a lower bound and never as the bound itself (S2).
 *
 * The video panel's floor is not among them any more. It is what the transport under the panel
 * asks for on one row, which is a width the machine's fonts decide, so it is asserted here as the
 * property it exists for and never as a number: the row is one row at the floor, and the seek bar
 * in it is down to the width its own rule floors it at.
 */
const MIN_TOOLS_WIDTH = 176;
const MIN_GRID_HEIGHT = 109;
const MIN_CURRENT_LINE = 72;

/**
 * What a width read back off the page may be over the width it was clamped to: the floor is rounded
 * up to a whole pixel, and a share of a row stored and multiplied back lands on a fraction of one.
 */
const ROUNDING_PX = 2;

/** What the two edges open at: 38% of the top row, and 13.5rem at the default root size. */
const DEFAULT_VIDEO_FRACTION = 0.38;
const DEFAULT_TOP_HEIGHT = 216;

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

async function boxOf(selector) {
  const rect = await rectOf(selector);
  if (rect === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  return { width: rect.cssWidth, height: rect.cssHeight };
}

/** Every number the two edges trade, read in one round trip so they all describe one layout. */
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
      status: height(".statusbar"),
      statusTop: document.querySelector(".statusbar")?.getBoundingClientRect().top ?? null,
    };
  });
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
  const fromX = toplevel.absX + sash.midX;
  const fromY = toplevel.absY + sash.midY;
  const toX = toplevel.absX + inside(sash.midX + dx, toplevel.width);
  const toY = toplevel.absY + inside(sash.midY + dy, toplevel.height);
  dragAt(fromX, fromY, toX, toY);
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

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

/**
 * The seek bar as it is drawn, beside the width its own rule says it never goes under. The video
 * panel's floor is the transport with the bar at exactly that, so the two are equal there and the
 * bar is wider everywhere else.
 */
async function seekBar() {
  const bar = await browser.execute(() => {
    const slider = document.querySelector(".controls__slider");
    if (slider === null) {
      return null;
    }
    return {
      width: slider.getBoundingClientRect().width,
      minimum: Number.parseFloat(window.getComputedStyle(slider).minWidth),
    };
  });
  if (bar === null || !Number.isFinite(bar.minimum)) {
    throw new Error(".controls__slider is missing, or its rule gives it no width to hold it at");
  }
  return bar;
}

/**
 * The seek bar's slack while the transport shows its other reading. The button's two words are not
 * the same width and the floor keeps room for the wider of them, so the row is only full in one of
 * the two and both have to be read. Playback is what puts the other word up, and the video is left
 * paused, the way it was found.
 */
async function slackInTheOtherReading(toplevel) {
  const paused = await textOf(".controls__button");
  const saysSomethingElse = async () => ((await textOf(".controls__button")) === paused ? null : 1);
  await clickElement(toplevel, ".controls__button");
  await waitFor(saysSomethingElse, {
    timeout: 10000,
    message: "the transport button to show its other word, which is what this reading needs",
  });
  const bar = await seekBar();
  await clickElement(toplevel, ".controls__button");
  await waitFor(async () => ((await saysSomethingElse()) === null ? 1 : null), {
    timeout: 10000,
    message: "the transport button to go back to the word it showed before this check played it",
  });
  return bar.width - bar.minimum;
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

/** A document in the grid and a video on the stage: both edges are between panels that hold both. */
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

describe("the shell's three edges", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    toplevel = await attachToApp();
    await openTheFixtures(toplevel);
  });

  // The store is shared with every other spec in the run, and `waveform-sash.spec.js` opens by
  // asserting its panel is at the default height. This file drags three edges, so it puts the
  // store back the way it found it.
  after(() => {
    rmSync(storedLayout(), { force: true });
  });

  it("opens with all three edges at their defaults", async () => {
    const sizes = await shellSizes();
    expect(sizes.video / sizes.top).toBeCloseTo(DEFAULT_VIDEO_FRACTION, 2);
    expect(sizes.block).toBe(DEFAULT_TOP_HEIGHT);
    expect((await boxOf(".waveform")).height).toBe(128);
  });

  it("makes the video wider and the tools column narrower, and leaves the grid where it is", async () => {
    const before = await shellSizes();

    await dragSash(toplevel, VIDEO_SASH, 120, 0);

    const after = await shellSizes();
    expect(after.video).toBeGreaterThan(before.video);
    expect(after.tools).toBeLessThan(before.tools);
    expect(after.block).toBe(before.block);
    expect(after.grid).toBe(before.grid);
    expect(after.statusTop).toBe(before.statusTop);
    // The row is not stretched: what one column takes the other gives.
    expect(after.top).toBe(before.top);
  });

  it("makes the video narrower again when the same edge is dragged back", async () => {
    const before = await shellSizes();

    await dragSash(toplevel, VIDEO_SASH, -80, 0);

    const after = await shellSizes();
    expect(after.video).toBeLessThan(before.video);
    expect(after.tools).toBeGreaterThan(before.tools);
    expect(after.grid).toBe(before.grid);
  });

  it("widens the video while the button is still down, not only when it is let go", async () => {
    // Why this exists: emptying the live resize left every other assertion in this file green,
    // because they all read a settled layout after the release. A panel that ignores the pointer
    // and jumps once it is let go passes all of them, and looks broken to the person dragging it.
    const before = await shellSizes();
    const sash = await rectOf(VIDEO_SASH);
    if (sash === null) {
      throw new Error(`${VIDEO_SASH} is missing from the DOM, so there is nothing to drag`);
    }
    // Toward the wider of the two panels, so the read is never taken against a bound the drag
    // stopped at: both floors are well under half the row, so the wider panel has this much to give.
    const travel = before.video >= before.tools ? -140 : 140;
    const fromX = toplevel.absX + sash.midX;
    const fromY = toplevel.absY + sash.midY;

    pressAndTravel(fromX, fromY, fromX + travel, fromY);
    try {
      const moved = await waitFor(
        async () => {
          const now = (await shellSizes()).video;
          return Math.abs(now - before.video) > 100 ? now : null;
        },
        {
          timeout: 5000,
          message: `the video panel to move more than 100px from ${Math.round(before.video)} while the button is still down`,
        },
      );
      // The direction as well as the distance: a panel that jumped the other way is not following.
      expect(travel < 0 ? moved < before.video : moved > before.video).toBe(true);
    } finally {
      // Never leave the button down: it lands on whatever the next check clicks.
      releaseButton();
    }
    await browser.pause(250);
  });

  it("stops the video edge at a floor and a ceiling where both panels are still usable", async () => {
    // The ceiling first: the transport's height with room to spare is read off this run, and the
    // floor below is asked to match it. Nothing here stands in for one row.
    await dragSash(toplevel, VIDEO_SASH, 2000, 0);
    const atCeiling = await shellSizes();
    const oneRow = (await boxOf(".controls")).height;
    expect(atCeiling.tools).toBeGreaterThanOrEqual(MIN_TOOLS_WIDTH - 1);
    // Still a current line, not a sliver.
    expect((await boxOf(".currentline")).height).toBeGreaterThanOrEqual(MIN_CURRENT_LINE);

    await dragSash(toplevel, VIDEO_SASH, -2000, 0);
    const atFloor = await shellSizes();
    expect(atFloor.video).toBeLessThan(atCeiling.video);
    // Still a transport, not a sliver: at the floor the row is the height it is when the panel has
    // room, so it is on the one row and has not wrapped onto a second.
    expect((await boxOf(".controls")).height).toBe(oneRow);
    // Still a seek bar, at the width its own rule holds it at, which is what the floor keeps room
    // for: a floor that had been the row without it would leave nothing to put a pointer on.
    const bar = await seekBar();
    expect(bar.width).toBeGreaterThanOrEqual(bar.minimum);
    // And no wider than one row needs: in whichever of its two readings the transport is widest,
    // the row at the floor is full. A floor a hand had made generous leaves the slack here. The
    // media runs under ten minutes, so the position beside the duration reads the same width.
    const slack = Number(
      Math.min(bar.width - bar.minimum, await slackInTheOtherReading(toplevel)).toFixed(2),
    );
    expect({ slack, full: slack < ROUNDING_PX }).toEqual({ slack, full: true });

    // Left well clear of the default, so the check that all three are remembered can see it.
    await dragSash(toplevel, VIDEO_SASH, -2000, 0);
    await dragSash(toplevel, VIDEO_SASH, 200, 0);
  });

  it("grows the grid and shrinks the top block, and the video keeps its width", async () => {
    const before = await shellSizes();

    await dragSash(toplevel, GRID_SASH, 0, -60);

    const after = await shellSizes();
    expect(after.block).toBeLessThan(before.block);
    expect(after.grid).toBeGreaterThan(before.grid);
    expect(after.video).toBe(before.video);
    expect(after.statusTop).toBe(before.statusTop);
    // The block and the grid trade with each other and with nothing else.
    expect(Math.round(after.block + after.grid)).toBe(Math.round(before.block + before.grid));
  });

  it("stops the grid edge at a floor and a ceiling where both panels are still usable", async () => {
    await dragSash(toplevel, GRID_SASH, 0, -2000);
    const atFloor = await shellSizes();
    // The floor is measured off the current line's own slack, so it lands where the line reaches
    // its minimum rather than at a number: the waveform above it keeps the height it was left at.
    expect(atFloor.line).toBeGreaterThanOrEqual(MIN_CURRENT_LINE - 1);
    expect((await boxOf(".controls")).height).toBeGreaterThan(0);

    await dragSash(toplevel, GRID_SASH, 0, 2000);
    const atCeiling = await shellSizes();
    expect(atCeiling.block).toBeGreaterThan(atFloor.block);
    expect(atCeiling.grid).toBeGreaterThanOrEqual(MIN_GRID_HEIGHT - 1);
    // A grid at its floor still shows its header and rows under it, not a header alone.
    expect(await present(".cuelist__row")).toBe(true);

    await dragSash(toplevel, GRID_SASH, 0, -2000);
    await dragSash(toplevel, GRID_SASH, 0, 70);
  });

  it("moves the native surface with the video edge", async () => {
    await dragSash(toplevel, VIDEO_SASH, -70, 0);
    await waitForSurfaceOnStage(toplevel);

    await dragSash(toplevel, VIDEO_SASH, 70, 0);
    await waitForSurfaceOnStage(toplevel);
  });

  it("moves the native surface with the grid edge", async () => {
    await dragSash(toplevel, GRID_SASH, 0, -50);
    await waitForSurfaceOnStage(toplevel);

    await dragSash(toplevel, GRID_SASH, 0, 50);
    await waitForSurfaceOnStage(toplevel);
  });

  it("opens all three edges where they were left", async () => {
    const left = await shellSizes();
    const waveform = (await boxOf(".waveform")).height;
    expect(left.video / left.top).not.toBeCloseTo(DEFAULT_VIDEO_FRACTION, 2);
    expect(left.block).not.toBe(DEFAULT_TOP_HEIGHT);

    await browser.execute(() => {
      window.name = "before-the-relaunch";
    });
    await browser.reloadSession();
    toplevel = await attachToApp();
    expect(await browser.execute(() => window.name)).not.toBe("before-the-relaunch");
    await openTheFixtures(toplevel);

    const reopened = await shellSizes();
    expect(reopened.block).toBe(left.block);
    expect(reopened.video / reopened.top).toBeCloseTo(left.video / left.top, 2);
    expect((await boxOf(".waveform")).height).toBe(waveform);
  });

  it("opens all three at their defaults when the stored layout cannot be read", async () => {
    // Written between the two sessions, so the next launch is the one that meets it. The backend
    // says so in its log and the shell shows nothing: an unreadable layout costs one drag.
    writeFileSync(storedLayout(), '{"videoFraction": ');

    await browser.reloadSession();
    toplevel = await attachToApp();
    await openTheFixtures(toplevel);

    const sizes = await shellSizes();
    expect(sizes.video / sizes.top).toBeCloseTo(DEFAULT_VIDEO_FRACTION, 2);
    expect(sizes.block).toBe(DEFAULT_TOP_HEIGHT);
    expect((await boxOf(".waveform")).height).toBe(128);
  });

  it("opens all three at their defaults when there is no stored layout at all", async () => {
    // The other half of the same claim: a deleted store is a first launch, not an error.
    rmSync(storedLayout(), { force: true });

    await browser.reloadSession();
    toplevel = await attachToApp();
    await openTheFixtures(toplevel);

    const sizes = await shellSizes();
    expect(sizes.video / sizes.top).toBeCloseTo(DEFAULT_VIDEO_FRACTION, 2);
    expect(sizes.block).toBe(DEFAULT_TOP_HEIGHT);
    expect((await boxOf(".waveform")).height).toBe(128);
    await waitForSurfaceOnStage(toplevel);
  });
});
