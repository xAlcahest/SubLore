/* global describe, it, before, document, window, getComputedStyle, WheelEvent */
/**
 * The audio panel as the reference draws it: a ruler over the wave, a strip of controls under it,
 * and a window that goes where the current line is.
 *
 * Written against `docs/audio-panel-tasks.md` section 6. Everything here is read off what the app
 * draws: the ruler and the markers out of the two canvases' own pixels, the strip out of the DOM,
 * and the times out of the grid.
 *
 * The gestures avoid pressing on the wave itself. A press on the body now retimes the current line
 * from scratch, which is the reference's own behaviour, so a click meant only to reach the panel
 * would be an edit. `waveform-timing.spec.js` owns the presses.
 */
import { copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { repoRoot, requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { closeAnyOpenProject } from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

const OPEN_STATUS = "SRT · 3 cues · LF";

/** The fixture's three cues, committed and byte-frozen, so these are facts. */
const CUES = [
  { start: "00:00:02.120", end: "00:00:04.880" },
  { start: "00:00:05.000", end: "00:00:08.340" },
  { start: "00:00:09.100", end: "00:00:11.760" },
];

/** The strip, left to right, as `docs/audio-panel-tasks.md` A10 writes it down. */
const STRIP = [
  "time-prev-cue",
  "time-next-cue",
  "wave-play-selection",
  "time-play-line",
  "wave-stop",
  "time-play-before",
  "time-play-after",
  "wave-play-first",
  "wave-play-last",
  "time-play-to-end",
  "time-lead-in",
  "time-lead-out",
  "wave-center-on-cue",
  "wave-toggle-autoscroll",
];

/** How many dividers the five groups above put between them. */
const DIVIDERS = 4;

/** How far the two leads move a boundary. Mirrors `LEAD_IN_MS` and `LEAD_OUT_MS` in src/App.tsx. */
const LEAD_IN_MS = 100;
const LEAD_OUT_MS = 350;

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
  const directory = path.join(dataHome(), "waveform-panel");
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
 * Whether both of the cursor's line's boundaries are on the panel, read at the middle row of the
 * backing store for the reason `waveform-timing.spec.js` gives: the feet paint the top row.
 */
function bothMarkersDrawn() {
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
    const middle = Math.floor(canvas.height / 2);
    const row = canvas.getContext("2d").getImageData(0, middle, canvas.width, 1).data;
    const holds = (want) => {
      for (let x = 0; x < canvas.width; x += 1) {
        const at = x * 4;
        const off =
          Math.abs(row[at] - want[0]) +
          Math.abs(row[at + 1] - want[1]) +
          Math.abs(row[at + 2] - want[2]);
        if (off <= 6) {
          return true;
        }
      }
      return false;
    };
    return holds(rgb("--marker-start")) && holds(rgb("--marker-end"));
  });
}

/** Whether the current line's own span is drawn on a background of its own (A6). */
function currentLineTinted() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const root = getComputedStyle(document.documentElement);
    const hex = root.getPropertyValue("--wave-current").trim().replace("#", "");
    const want = [0, 2, 4].map((at) => parseInt(hex.slice(at, at + 2), 16));
    // The top row: the tint runs the full height and nothing else but the feet is painted there.
    const row = canvas.getContext("2d").getImageData(0, 0, canvas.width, 1).data;
    let columns = 0;
    for (let x = 0; x < canvas.width; x += 1) {
      const at = x * 4;
      const off =
        Math.abs(row[at] - want[0]) +
        Math.abs(row[at + 1] - want[1]) +
        Math.abs(row[at + 2] - want[2]);
      if (off <= 6) {
        columns += 1;
      }
    }
    return columns;
  });
}

/**
 * One row of the ruler's own canvas, as a string, so "the ruler changed" is answerable.
 *
 * The row read is the one directly above the band's own bottom rule, which the drawing paints as
 * many device pixels thick as the display is scaled by. Reading the rule itself would be reading a
 * line that is the full width at every zoom, which is the one row that can never change.
 */
function rulerSignature() {
  return browser.execute(() => {
    const band = document.querySelector(".waveform__ruler");
    if (band === null || band.width === 0 || band.height === 0) {
      return null;
    }
    const rule = Math.max(1, Math.round(window.devicePixelRatio));
    const row = band.getContext("2d").getImageData(0, band.height - 1 - rule, band.width, 1).data;
    let out = "";
    for (let x = 0; x < band.width; x += 1) {
      out += row[x * 4] > 90 ? "1" : "0";
    }
    return out;
  });
}

/** How tall the ruler band is, in device pixels, as the drawing itself sized it. */
function rulerHeight() {
  return browser.execute(() => document.querySelector(".waveform__ruler")?.height ?? null);
}

/** A wheel notch over the canvas, at its centre, with or without ctrl. Negative zooms or scrolls
 * towards the start of the media. */
async function wheelOverCanvas(notches, ctrl) {
  await browser.execute(
    (count, withCtrl) => {
      const canvas = document.querySelector(".waveform__canvas");
      const box = canvas.getBoundingClientRect();
      for (let step = 0; step < Math.abs(count); step += 1) {
        canvas.dispatchEvent(
          new WheelEvent("wheel", {
            bubbles: true,
            cancelable: true,
            clientX: box.x + box.width / 2,
            clientY: box.y + box.height / 2,
            deltaY: count > 0 ? 100 : -100,
            ctrlKey: withCtrl,
          }),
        );
      }
    },
    notches,
    ctrl,
  );
  await browser.pause(150);
}

/** Put the cursor on a one-based row of the grid, by clicking its number cell. */
async function cursorToRow(toplevel, number) {
  const centre = await browser.execute((wanted) => {
    const cell = Array.from(document.querySelectorAll(".cuelist__row"))
      .find((row) => row.querySelector(".cuelist__pos")?.textContent === wanted)
      ?.querySelector(".cuelist__pos");
    if (!cell) {
      return null;
    }
    const rect = cell.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, String(number));
  if (centre === null) {
    throw new Error(`row ${number} is missing from the grid`);
  }
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
  await waitFor(async () => ((await gridRows())[number - 1]?.cursor === true ? true : null), {
    timeout: 15000,
    message: `the cursor to reach row ${number}`,
  });
}

/** Open one of View's items, which is where the panel's two window commands live. */
async function runViewItem(toplevel, token) {
  await clickElement(toplevel, ".menubar__title--view");
  await waitFor(() => present(`.menubar__item--${token}`), {
    timeout: 15000,
    message: `the View dropdown to open on ${token}`,
  });
  await clickElement(toplevel, `.menubar__item--${token}`);
  await waitFor(async () => ((await present(".menubar__menu")) ? null : true), {
    timeout: 15000,
    message: "the View dropdown to close behind the item",
  });
  await browser.pause(150);
}

describe("the waveform panel's ruler, strip and window", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    const copy = workingCopy();
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
    await answerChooser(await waitForChooser("Choose a subtitle"), copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__document"))?.includes(OPEN_STATUS) === true,
      { timeout: 20000, message: "the status bar to report the open subtitle" },
    );

    await clickElement(toplevel, ".toolbar__video-open");
    await answerChooser(await waitForChooser("Choose a video"), requireWaveformFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(() => present(".waveform"), {
      timeout: 40000,
      message: "the waveform panel to appear",
    });
  });

  it("draws a ruler band over the wave, sized by its own type", async () => {
    expect(await present(".waveform__ruler")).toBe(true);
    // Measured off the type the band draws, never written down, so all this can say is that it is
    // a band rather than nothing and that it is not taller than the panel it sits in.
    const band = await rulerHeight();
    expect(band).toBeGreaterThan(0);
    const panel = await browser.execute(
      () => document.querySelector(".waveform__canvas")?.height ?? 0,
    );
    expect(band).toBeLessThan(panel);

    const marks = await rulerSignature();
    expect(marks).not.toBe(null);
    // Ticks, not an empty strip: the row just above the band's own bottom line carries them.
    expect(marks.includes("1")).toBe(true);
  });

  it("changes the ruler's marks when the zoom changes", async () => {
    const whole = await rulerSignature();
    await wheelOverCanvas(-4, true);
    const deep = await rulerSignature();
    expect(`the ruler moved with the zoom: ${deep !== whole}`).toBe(
      "the ruler moved with the zoom: true",
    );
    await wheelOverCanvas(4, true);
  });

  it("brings the current line's audio on screen when the cursor moves onto it", async () => {
    // The document opens with the cursor on row 1, so it is put on another row first: a click on
    // the row the cursor is already on is not a move, and the panel follows moves.
    await cursorToRow(toplevel, 2);
    // Deep enough that no window holds the first cue and the last one at once: the fixture's cues
    // are at 2.1 to 4.9 seconds and 9.1 to 11.8, and four steps in leaves well under seven seconds
    // on the panel.
    await wheelOverCanvas(-4, true);

    await cursorToRow(toplevel, 1);
    await waitFor(async () => ((await bothMarkersDrawn()) ? true : null), {
      timeout: 15000,
      message: "the first cue's boundaries to be brought onto the panel",
    });

    await cursorToRow(toplevel, 3);
    await waitFor(async () => ((await bothMarkersDrawn()) ? true : null), {
      timeout: 15000,
      message: "the last cue's boundaries to be brought onto the panel",
    });
  });

  it("draws the current line's own span on a background of its own", async () => {
    // The cursor is on the last cue and the panel is on it, from the test above. The last cue is
    // the one this can be read on: the fixture's tone blocks reach the top of the panel, so a tint
    // under one of them is painted over, and 9.1 to 11.8 seconds crosses into the silent block.
    const columns = await currentLineTinted();
    expect(`the line's span is tinted: ${columns > 0}`).toBe("the line's span is tinted: true");
  });

  it("centres on the line from the command, and stops following when the toggle is off", async () => {
    // Away down the media, by the wheel: a click on the body would retime the line instead.
    await wheelOverCanvas(40, false);
    expect(`the panel is off the line: ${await bothMarkersDrawn()}`).toBe(
      "the panel is off the line: false",
    );

    await runViewItem(toplevel, "wave-center-on-cue");
    expect(`the command brought it back: ${await bothMarkersDrawn()}`).toBe(
      "the command brought it back: true",
    );

    // Following off: the cursor moves and the window does not.
    await runViewItem(toplevel, "wave-toggle-autoscroll");
    await cursorToRow(toplevel, 1);
    expect(`the window stayed put: ${await bothMarkersDrawn()}`).toBe(
      "the window stayed put: false",
    );

    // And back on, which is the barrier the absence above needs: the same panel, the same cursor.
    await runViewItem(toplevel, "wave-toggle-autoscroll");
    await waitFor(async () => ((await bothMarkersDrawn()) ? true : null), {
      timeout: 15000,
      message: "the panel to follow the line again once the toggle is back on",
    });
    await wheelOverCanvas(4, true);
  });

  it("draws its own controls under the wave, in the reference's order", async () => {
    const drawn = await browser.execute(() =>
      Array.from(document.querySelectorAll(".wavebar__button")).map((button) => ({
        id:
          Array.from(button.classList)
            .find((name) => name !== "wavebar__button")
            ?.replace("wavebar__", "") ?? null,
        disabled: button.disabled,
        pressed: button.getAttribute("aria-pressed"),
      })),
    );
    expect(drawn.map((button) => button.id)).toEqual(STRIP);
    expect(await browser.execute(() => document.querySelectorAll(".wavebar__divider").length)).toBe(
      DIVIDERS,
    );

    // Inside the panel and under the wave, never above it and never outside it (A11).
    expect(
      await browser.execute(() => {
        const bar = document.querySelector(".wavebar");
        const wave = document.querySelector(".waveform__canvas");
        if (bar === null || wave === null) {
          return null;
        }
        return {
          insidePanel: document.querySelector(".waveform")?.contains(bar) ?? false,
          underTheWave: bar.getBoundingClientRect().top >= wave.getBoundingClientRect().bottom,
        };
      }),
    ).toEqual({ insidePanel: true, underTheWave: true });

    // A video is open and a cue carries the cursor, so the ear controls run. Stop is the one that
    // is greyed anyway: nothing is playing.
    expect(await disabledOf(".wavebar__time-play-line")).toBe(false);
    expect(await disabledOf(".wavebar__wave-play-first")).toBe(false);
    expect(await disabledOf(".wavebar__wave-stop")).toBe(true);
    // The follow toggle draws pressed while it is on, the way a toolbar toggle does.
    expect(drawn.find((button) => button.id === "wave-toggle-autoscroll").pressed).toBe("true");
  });

  it("adds lead-in and lead-out from the strip, each in one undo step", async () => {
    await cursorToRow(toplevel, 2);
    const before = (await gridRows())[1];
    expect([before.start, before.end]).toEqual([CUES[1].start, CUES[1].end]);

    await clickElement(toplevel, ".wavebar__time-lead-in");
    const afterIn = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.start !== before.start ? now : null;
      },
      { timeout: 20000, message: "lead-in to pull the second cue's start back" },
    );
    expect(asMillis(before.start) - asMillis(afterIn[1].start)).toBe(LEAD_IN_MS);
    expect(afterIn[1].end).toBe(before.end);

    await clickElement(toplevel, ".wavebar__time-lead-out");
    const afterOut = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.end !== before.end ? now : null;
      },
      { timeout: 20000, message: "lead-out to push the second cue's end on" },
    );
    expect(asMillis(afterOut[1].end) - asMillis(before.end)).toBe(LEAD_OUT_MS);
    expect(afterOut[1].start).toBe(afterIn[1].start);

    // One undo each, which is what a boundary move is worth.
    await clickElement(toplevel, ".toolbar__edit-undo");
    await waitFor(async () => ((await gridRows())[1]?.end === before.end ? true : null), {
      timeout: 20000,
      message: "one undo to take the lead-out back",
    });
    await clickElement(toplevel, ".toolbar__edit-undo");
    const back = await waitFor(
      async () => ((await gridRows())[1]?.start === before.start ? true : null),
      { timeout: 20000, message: "one more undo to take the lead-in back" },
    ).then(() => gridRows());
    expect([back[1].start, back[1].end]).toEqual([CUES[1].start, CUES[1].end]);
  });
});
