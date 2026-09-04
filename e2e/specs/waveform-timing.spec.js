/* global describe, it, before, document, window, getComputedStyle, PointerEvent */
/**
 * M2.5: the cursor's cue puts two markers on the waveform and either can be dragged.
 *
 * The drag is a real press, travel and release through X11, the way `waveform-sash.spec.js` drags
 * the panel's own edge: a pointer event dispatched into the page would exercise React and prove
 * nothing about whether a hand can put a pointer on a boundary and pull it.
 *
 * Where a marker is drawn is read out of the canvas's own pixels, the way `waveform-view.spec.js`
 * reads them, because the marker is a rectangle on a canvas and there is no element to query. The
 * two tokens have colours of their own, so a column drawn in one of them is that boundary and not
 * the wave, the playhead or the background.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, dragAt, focusWindow } from "../lib/input.js";
import { takeCommands, watchCommands } from "../lib/ipc.js";
import { repoRoot, requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { closeAnyOpenProject } from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

const OPEN_STATUS = "SRT · 3 cues · LF";

/** The fixture's second cue, committed and byte-frozen, so these are facts. */
const SECOND_START = "00:00:05.000";
const SECOND_END = "00:00:08.340";

/** Mirrors `GRAB_PX` in src/components/Waveform.tsx: the hit area either side of a marker. */
const GRAB_COLUMNS = 8;

function dataHome() {
  const home = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof home !== "string" || home === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  return home;
}

/** Writes go to the harness temp dir. The committed fixture is copied, never opened for editing. */
function workingCopy() {
  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  if (!existsSync(source)) {
    throw new Error(
      `E2E prerequisite missing: ${source} does not exist. It is committed; restore it with ` +
        "`git checkout fixtures/subtitles`.",
    );
  }
  const directory = path.join(dataHome(), "waveform-timing");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(source, copy);
  return copy;
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function disabledOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.disabled ?? null, selector);
}

async function clickElement(toplevel, selector) {
  const centre = await browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

/** Every row the grid draws, with the cursor marked, read in one round trip. */
function gridRows() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__row")).map((row) => ({
      start: row.querySelector(".cuelist__start")?.textContent ?? null,
      end: row.querySelector(".cuelist__end")?.textContent ?? null,
      cursor: row.classList.contains("cuelist__row--active"),
    })),
  );
}

/** A timecode the grid drew, as the milliseconds the product reasons in (decision 11). */
function asMillis(timecode) {
  const parts = /^(\d+):(\d+):(\d+)\.(\d+)$/.exec(timecode ?? "");
  if (parts === null) {
    throw new Error(`"${timecode}" is not a timecode the grid draws`);
  }
  return (
    Number(parts[1]) * 3_600_000 +
    Number(parts[2]) * 60_000 +
    Number(parts[3]) * 1000 +
    Number(parts[4])
  );
}

/**
 * The first column drawn in each marker's own colour, and the panel's width, in device pixels.
 *
 * One row of the backing store is enough: a marker runs the full height, so any row it crosses
 * carries it. The colours come from the tokens the drawing reads, never from a copy here.
 */
function markerColumns() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const root = getComputedStyle(document.documentElement);
    const rgb = (name) => {
      const hex = root.getPropertyValue(name).trim().replace("#", "");
      return [0, 2, 4].map((at) => parseInt(hex.slice(at, at + 2), 16));
    };
    const row = canvas.getContext("2d").getImageData(0, 0, canvas.width, 1).data;
    const find = (want) => {
      for (let x = 0; x < canvas.width; x += 1) {
        const at = x * 4;
        const off =
          Math.abs(row[at] - want[0]) +
          Math.abs(row[at + 1] - want[1]) +
          Math.abs(row[at + 2] - want[2]);
        if (off <= 6) {
          return x;
        }
      }
      return null;
    };
    return {
      start: find(rgb("--marker-start")),
      end: find(rgb("--marker-end")),
      width: canvas.width,
    };
  });
}

/** Where the canvas sits on screen, in the device pixels its columns are counted in. */
function canvasBox() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: rect.x * dpr, midY: (rect.y + rect.height / 2) * dpr };
  });
}

/** The media's own length in milliseconds, off the transport rather than out of a constant. */
async function durationMs() {
  const seconds = await browser.execute(() =>
    Number(document.querySelector(".controls__slider")?.max ?? 0),
  );
  if (seconds <= 0) {
    throw new Error(
      "the transport reports no duration, so the panel has no scale to check against",
    );
  }
  return seconds * 1000;
}

/** Press at one column of the panel, travel to another and let go. */
async function dragColumns(toplevel, fromColumn, byColumns) {
  const box = await canvasBox();
  if (box === null) {
    throw new Error(".waveform__canvas is missing from the DOM, so there is nothing to drag");
  }
  const y = toplevel.absY + box.midY;
  const fromX = toplevel.absX + box.x + fromColumn;
  dragAt(fromX, y, fromX + byColumns, y);
  // The release is what reaches the document, so this reads a settled panel, never a mid-drag one.
  await browser.pause(400);
}

/**
 * The shape the pointer takes at a column, from a dispatched move.
 *
 * The one gesture here that is not X11, and deliberately: what is asserted is the inline style the
 * handler writes, the X11 tests below already prove a hand can grab what this shape advertises, and
 * `xdotool` cannot be asked what the cursor looks like.
 */
function cursorAt(column) {
  return browser.execute((at) => {
    const canvas = document.querySelector(".waveform__canvas");
    const box = canvas.getBoundingClientRect();
    canvas.dispatchEvent(
      new PointerEvent("pointermove", {
        bubbles: true,
        clientX: box.x + at / window.devicePixelRatio,
        clientY: box.y + box.height / 2,
        pointerId: 1,
      }),
    );
    return canvas.style.cursor;
  }, column);
}

async function cursorToSecondRow(toplevel) {
  const centre = await browser.execute(() => {
    const cell = Array.from(document.querySelectorAll(".cuelist__row"))
      .find((row) => row.querySelector(".cuelist__pos")?.textContent === "2")
      ?.querySelector(".cuelist__pos");
    if (!cell) {
      return null;
    }
    const rect = cell.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  });
  if (centre === null) {
    throw new Error("the second row is missing from the DOM");
  }
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
  await waitFor(async () => ((await gridRows())[1]?.cursor === true ? true : null), {
    timeout: 15000,
    message: "the cursor to reach the second row",
  });
}

describe("dragging a cue boundary on the waveform", () => {
  let toplevel = null;
  let copy = null;
  let openedBytes = null;
  /** Milliseconds per device pixel at the zoom the panel opens on: the whole media across it. */
  let msPerColumn = 0;
  let columns = null;
  /**
   * How far a drag travels and where a press lands that grabs nothing, both taken off the panel.
   *
   * A fixed count of columns would be a number about one layout: the tools column is a share of the
   * window and three other specs leave it at whatever they dragged it to, so half the distance
   * between the two markers is the travel that stays inside the cue at any width.
   */
  let travel = 0;
  let nowhereNear = 0;

  before(async () => {
    requireWaveformFixture();
    copy = workingCopy();
    openedBytes = readFileSync(copy);
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
    // One data home for the whole run, so the emptiest state is one this file makes. See N19.
    await closeAnyOpenProject(toplevel);

    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const subtitle = await waitForChooser("Choose a subtitle");
    await answerChooser(subtitle, copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__document"))?.includes(OPEN_STATUS) === true,
      {
        timeout: 20000,
        message: "the status bar to report the open subtitle",
      },
    );

    await clickElement(toplevel, ".toolbar__video-open");
    const video = await waitForChooser("Choose a video");
    await answerChooser(video, requireWaveformFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(() => present(".waveform"), {
      timeout: 40000,
      message: "the waveform panel to appear",
    });

    await cursorToSecondRow(toplevel);
    columns = await waitFor(
      async () => {
        const found = await markerColumns();
        return found !== null && found.start !== null && found.end !== null ? found : null;
      },
      { timeout: 20000, message: "both of the second cue's markers to be drawn" },
    );
    msPerColumn = (await durationMs()) / columns.width;
    travel = Math.max(3, Math.floor((columns.end - columns.start) / 2));
    nowhereNear = Math.min(columns.width - 3, columns.end + GRAB_COLUMNS * 3);
  });

  it("draws the cursor's cue's two boundaries where the cue is", async () => {
    // Within one column: the panel opens on the whole media, so a column is the finest thing it
    // can say, and the scale comes from the panel's own width and the media's own length.
    expect(Math.abs(columns.start * msPerColumn - asMillis(SECOND_START))).toBeLessThan(
      msPerColumn,
    );
    expect(Math.abs(columns.end * msPerColumn - asMillis(SECOND_END))).toBeLessThan(msPerColumn);
    expect(columns.end).toBeGreaterThan(columns.start);
  });

  it("takes the resize shape only where a press would grab", async () => {
    // The press that grabs nothing is checked to be outside every hit area rather than assumed to
    // be: it is derived from a panel three other specs can leave at a different width.
    expect(nowhereNear - columns.end).toBeGreaterThan(GRAB_COLUMNS);
    expect(await cursorAt(columns.start)).toBe("ew-resize");
    expect(await cursorAt(columns.end)).toBe("ew-resize");
    expect(`nowhere near a boundary: "${await cursorAt(nowhereNear)}"`).toBe(
      'nowhere near a boundary: ""',
    );
  });

  it("drags the start later and leaves the end and the file alone", async () => {
    await dragColumns(toplevel, columns.start, travel);

    const rows = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.start !== SECOND_START ? now : null;
      },
      { timeout: 20000, message: "the second row's start to follow the drag" },
    );
    // Where the pointer was let go, not merely somewhere later: one column and a half of slack for
    // the rounding a screen coordinate and a device pixel go through.
    const wanted = (columns.start + travel) * msPerColumn;
    expect(Math.abs(asMillis(rows[1].start) - wanted)).toBeLessThan(msPerColumn * 1.5);
    expect(rows[1].end).toBe(SECOND_END);
    expect(await present(".statusbar__dirty")).toBe(true);
    // A drag is an edit, and an edit is not a save.
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("takes the whole drag back in one undo", async () => {
    await clickElement(toplevel, ".toolbar__edit-undo");
    const rows = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.start === SECOND_START ? now : null;
      },
      { timeout: 20000, message: "one undo to put the second row's start back" },
    );
    expect(rows[1].end).toBe(SECOND_END);
    // Nothing left to undo: the whole travel was one history entry, not one per pointer move.
    expect(await disabledOf(".toolbar__edit-undo")).toBe(true);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("ignores a press that is nowhere near a boundary", async () => {
    await watchCommands();
    await dragColumns(toplevel, nowhereNear, -travel);
    // Nothing crossed the boundary, which is what "the press did nothing" has to mean.
    expect(await takeCommands()).toEqual([]);
    const rows = await gridRows();
    expect([rows[1].start, rows[1].end]).toEqual([SECOND_START, SECOND_END]);
  });

  it("refuses a drag that would leave the end at or before the start", async () => {
    // Past the start marker and out the other side, so the pair the release would ask for is one
    // no cue can have.
    const landing = Math.max(0, columns.start - travel);
    expect(landing).toBeLessThan(columns.start);

    await watchCommands();
    await dragColumns(toplevel, columns.end, landing - columns.end);
    expect(await takeCommands()).toEqual([]);
    const rows = await gridRows();
    expect([rows[1].start, rows[1].end]).toEqual([SECOND_START, SECOND_END]);
    // The marker went back where the document still has it rather than staying where it was let go.
    const after = await markerColumns();
    expect(after.end).toBe(columns.end);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });
});
