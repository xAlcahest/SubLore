/* global describe, it, before, document, window */
/**
 * T5: the current line in the tools column.
 *
 * "Committing there is the same operation `CueList` already performs" is the load-bearing half of
 * the criterion, so it is asserted as the command that crosses the IPC boundary and as the single
 * undo step that takes it back, not as "the text changed".
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** The fixture's own text and timings. It is committed and byte-frozen, so these are facts. */
const STATUS_PREFIX = "SRT · 3 cues · LF";
const FIRST = { text: "The harbour was empty when we got there.", start: "00:00:02.120" };
const THIRD = {
  text: "By then the fog had eaten the boats.",
  start: "00:00:09.100",
  end: "00:00:11.760",
  duration: "2.660",
};
/** Typed over the third cue from the tools column. */
const EDITED_TEXT = "Typed into the current line";

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
  const directory = path.join(dataHome(), "current-line");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(source, copy);
  return copy;
}

/** Centre of an element in physical pixels, which is what X11 pointer coordinates are. */
function centreOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, selector);
}

/** Centre of one cell of the row at a given 1-based list position, if that row is rendered. */
function centreOfCell(position, cell) {
  return browser.execute(
    (wanted, css) => {
      const rows = Array.from(document.querySelectorAll(".cuelist__row"));
      const row = rows.find(
        (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
      );
      const target = row?.querySelector(css);
      if (!target) {
        return null;
      }
      const rect = target.getBoundingClientRect();
      const dpr = window.devicePixelRatio;
      return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
    },
    String(position),
    cell,
  );
}

async function clickCentre(toplevel, centre, what) {
  if (centre === null) {
    throw new Error(`${what} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

async function clickElement(toplevel, selector) {
  await clickCentre(toplevel, await centreOf(selector), selector);
}

function key(name) {
  execFileSync("xdotool", ["key", "--clearmodifiers", name], { encoding: "utf8", timeout: 15000 });
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** Everything the box says about the row it is on, read in one round trip. */
function currentLine() {
  return browser.execute(() => {
    const box = document.querySelector(".currentline__text");
    const read = (css) => document.querySelector(css)?.textContent ?? null;
    return {
      text: box === null ? null : box.value,
      start: read(".currentline__start"),
      end: read(".currentline__end"),
      duration: read(".currentline__duration"),
      cps: read(".currentline__cps"),
      empty: read(".currentline__empty"),
    };
  });
}

/** What one grid row shows, so the two views of a cue can be read against each other. */
function gridRow(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    if (row === undefined) {
      return null;
    }
    const read = (css) => row.querySelector(css)?.textContent ?? null;
    return {
      text: read(".cuelist__text"),
      start: read(".cuelist__start"),
      end: read(".cuelist__end"),
      cps: read(".cuelist__cps"),
      cursor: row.classList.contains("cuelist__row--active"),
    };
  }, String(position));
}

/** Replace what the box holds, the way a person would: click it, select all, type. */
async function typeIntoBox(toplevel, text) {
  await clickElement(toplevel, ".currentline__text");
  await waitFor(
    () =>
      browser.execute(
        () => document.activeElement?.classList.contains("currentline__text") === true,
      ),
    { timeout: 15000, message: "the current-line box to take the keyboard" },
  );
  key("ctrl+a");
  typeText(text);
  await waitFor(async () => (await currentLine()).text === text, {
    timeout: 15000,
    message: `the current-line box to hold exactly ${text}`,
  });
}

describe("the current line", () => {
  let toplevel = null;
  let copy = null;
  let originalBytes = null;

  before(async () => {
    copy = workingCopy();
    originalBytes = readFileSync(copy);
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => present(".toolbar__open-subtitle"), {
      timeout: 30000,
      message: "the app UI to render",
    });
  });

  it("shows the line the cursor is on, and follows the cursor when it moves", async () => {
    // Nothing is open yet, so the box has no row and says so rather than drawing empty fields.
    const before = await currentLine();
    expect(before.text).toBe(null);
    expect(before.empty).not.toBe(null);

    await clickElement(toplevel, ".toolbar__open-subtitle");
    const chooser = await waitForChooser("Choose a subtitle");
    await answerChooser(chooser, copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__document"))?.includes(STATUS_PREFIX) === true,
      { timeout: 20000, message: "the status bar to report the open subtitle" },
    );

    // A document opens on its first row, so that is the row the box is on before anything is asked.
    const opened = await waitFor(
      async () => {
        const line = await currentLine();
        return line.text === null ? null : line;
      },
      { timeout: 15000, message: "the current-line box to appear over the open document" },
    );
    expect(opened.text).toBe(FIRST.text);
    expect(opened.start).toBe(FIRST.start);
    expect((await gridRow(1)).cursor).toBe(true);
    // The same cue drawn twice must read the same both times, whichever view is doing the drawing.
    expect(opened.cps).toBe((await gridRow(1)).cps);

    await clickCentre(toplevel, await centreOfCell(3, ".cuelist__pos"), "row 3 number cell");

    const moved = await waitFor(
      async () => {
        const line = await currentLine();
        return line.text === THIRD.text ? line : null;
      },
      { timeout: 15000, message: "the box to follow the cursor onto the third row" },
    );
    expect(moved.start).toBe(THIRD.start);
    expect(moved.end).toBe(THIRD.end);
    expect(moved.duration).toBe(THIRD.duration);
    expect(moved.cps).toBe((await gridRow(3)).cps);
    expect((await gridRow(3)).cursor).toBe(true);
    expect((await gridRow(1)).cursor).toBe(false);
  });

  it("commits through the command the grid commits with, and the grid row shows it", async () => {
    // Every subtitle command that crosses the boundary while the commit happens, in order. It goes
    // on `fetch`, which is what every command travels on, the way editor.spec.js measures the same
    // boundary: a second route into the document would show up here as a second name.
    await browser.execute(() => {
      window.__subloreCommands = [];
      const passThrough = window.fetch;
      window.__subloreFetch = passThrough;
      window.fetch = (...rest) => {
        const url = String(rest[0]?.url ?? rest[0]);
        const name = url.split("/").pop();
        if (typeof name === "string" && name.startsWith("subtitle_")) {
          window.__subloreCommands.push(name);
        }
        return passThrough.apply(window, rest);
      };
      if (window.fetch === passThrough) {
        throw new Error("the probe did not take: fetch is not writable here either");
      }
    });

    await typeIntoBox(toplevel, EDITED_TEXT);
    key("Return");

    await waitFor(async () => (await gridRow(3))?.text === EDITED_TEXT, {
      timeout: 20000,
      message: "the third grid row to show what the tools column committed",
    });
    const commands = await browser.execute(() => {
      window.fetch = window.__subloreFetch;
      return window.__subloreCommands;
    });

    expect(commands).toEqual(["subtitle_set_text"]);
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await present(".statusbar__error")).toBe(false);
    // The inline editor never opened: this edit was made in the tools column and nowhere else.
    expect(await present(".cuelist__editor")).toBe(false);
    // A commit is not a save, exactly as it is not one in the grid.
    expect(readFileSync(copy).equals(originalBytes)).toBe(true);
  });

  it("is undone in one step, which is what a grid edit costs", async () => {
    await clickElement(toplevel, ".toolbar__undo");

    await waitFor(async () => (await gridRow(3))?.text === THIRD.text, {
      timeout: 20000,
      message: "the third grid row to go back to the text the file was opened with",
    });
    // One step off the top of the stack put the document back where it opened. A commit that had
    // gone through a second operation, or through two, would leave something unsaved here.
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the unsaved marker to clear after a single undo",
    });
    expect(await present(".statusbar__error")).toBe(false);
    // The box holds the document again, not the text that was undone out from under it.
    expect((await currentLine()).text).toBe(THIRD.text);
    expect(readFileSync(copy).equals(originalBytes)).toBe(true);
  });
});
