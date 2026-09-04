/* global describe, it, before, document, window, Event */
/**
 * The five commands that tie the times to where the video is, driven from the Timing menu.
 *
 * Nothing here invokes a command: every one is run by opening the menu and clicking the item, which
 * is the only route a user has. What is asserted is the grid, the player's own readout, and the
 * bytes still on disk.
 *
 * The playhead's millisecond is READ rather than assumed. A seek asks for a time and the player
 * lands where it lands, so asserting a hardcoded 6500 would be asserting the seek's precision and
 * not the command's correctness. Every check below compares the cue against what the player says
 * about itself at that moment.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey } from "../lib/input.js";
import { takeCommands, watchCommands } from "../lib/ipc.js";
import { repoRoot, requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { closeAnyOpenProject } from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

const OPEN_STATUS = "SRT · 3 cues · LF";
/** The fixture's own third cue, committed and byte-frozen, so these are facts. */
const THIRD_START = "00:00:09.100";
const THIRD_END = "00:00:11.760";
/** Between the first cue's end at 4.880 and the second's start at 5.000: a gap, on purpose. */
const IN_THE_GAP = 4.94;
/** Inside the second cue, which runs 5.000 to 8.340. */
const INSIDE_SECOND = 6.5;

/** Every item the Timing menu draws, in the order the menu lists them. */
const TIMING_ITEMS = [
  "time-start-to-playhead",
  "time-end-to-playhead",
  "video-to-cue-start",
  "video-to-cue-end",
  "edit-select-at-playhead",
];

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
  const directory = path.join(dataHome(), "playhead");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(source, copy);
  return copy;
}

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

async function clickElement(toplevel, selector) {
  const centre = await centreOf(selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function disabledOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.disabled ?? null, selector);
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

/** Where the player says it is, in seconds, off its own transport rather than off any state. */
function playhead() {
  return browser.execute(() => Number(document.querySelector(".controls__slider")?.value ?? -1));
}

/** The same instant as the grid draws it, so the two can be compared as strings. */
function asTimecode(seconds) {
  const total = Math.max(0, Math.floor(seconds * 1000));
  const pad = (value, width) => String(value).padStart(width, "0");
  return (
    `${pad(Math.floor(total / 3_600_000), 2)}:` +
    `${pad(Math.floor(total / 60_000) % 60, 2)}:` +
    `${pad(Math.floor(total / 1000) % 60, 2)}.${pad(total % 1000, 3)}`
  );
}

async function openMenu(toplevel) {
  await clickElement(toplevel, ".menubar__title--timing");
  await waitFor(() => present(".menubar__menu"), {
    timeout: 15000,
    message: "the Timing menu to open",
  });
}

async function runFromMenu(toplevel, token) {
  await openMenu(toplevel);
  await clickElement(toplevel, `.menubar__item--${token}`);
  await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
    timeout: 15000,
    message: `the menu to close after ${token}`,
  });
}

async function seekTo(seconds) {
  await browser.execute((target) => {
    const slider = document.querySelector(".controls__slider");
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    setter.call(slider, String(target));
    slider.dispatchEvent(new Event("input", { bubbles: true }));
    slider.dispatchEvent(new Event("change", { bubbles: true }));
  }, seconds);
  await browser.pause(300);
}

/** Put the cursor on a row by clicking its number cell, which never opens an editor. */
async function cursorTo(toplevel, position) {
  const centre = await browser.execute((wanted) => {
    const row = Array.from(document.querySelectorAll(".cuelist__row")).find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    const cell = row?.querySelector(".cuelist__pos");
    if (!cell) {
      return null;
    }
    const rect = cell.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, String(position));
  if (centre === null) {
    throw new Error(`row ${position} is missing from the DOM`);
  }
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
  await waitFor(async () => ((await gridRows())[position - 1]?.cursor === true ? true : null), {
    timeout: 15000,
    message: `the cursor to reach row ${position}`,
  });
}

describe("the times follow the playhead", () => {
  let toplevel = null;
  let copy = null;
  let openedBytes = null;

  before(async () => {
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
  });

  it("draws all five greyed while no video is open, and running one does nothing", async () => {
    await openMenu(toplevel);
    for (const token of TIMING_ITEMS) {
      expect({
        token,
        drawn: await present(`.menubar__item--${token}`),
        disabled: await disabledOf(`.menubar__item--${token}`),
      }).toEqual({ token, drawn: true, disabled: true });
    }

    await watchCommands();
    await clickElement(toplevel, `.menubar__item--${TIMING_ITEMS[0]}`);
    await browser.pause(500);
    // Nothing crossed the boundary, which is what "greyed" has to mean and not just how it looks.
    expect(await takeCommands()).toEqual([]);
    // XTEST, not WebDriver: the Actions endpoint answers "unsupported operation" against a wry
    // webview, which is why this harness has its own input layer. See e2e/README.md.
    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the menu to close",
    });
  });

  it("sets the cursor's cue to start where the video is paused", async () => {
    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const subtitle = await waitForChooser("Choose a subtitle");
    await answerChooser(subtitle, copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__document"))?.includes(OPEN_STATUS) === true,
      { timeout: 20000, message: "the status bar to report the open subtitle" },
    );

    await clickElement(toplevel, ".toolbar__video-open");
    const video = await waitForChooser("Choose a video");
    await answerChooser(video, requireVideoFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(
      () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".controls__button")?.disabled === false,
        ),
      { timeout: 40000, message: "the video to be ready to play" },
    );

    await seekTo(INSIDE_SECOND);
    await cursorTo(toplevel, 2);
    // Read, not assumed: the seek asked for a time and the player landed where it landed.
    const paused = asTimecode(await playhead());

    await runFromMenu(toplevel, "time-start-to-playhead");
    const rows = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.start === paused ? now : null;
      },
      { timeout: 20000, message: `the second row to start at ${paused}` },
    );
    // Only the start moved: the end is the fixture's own, untouched.
    expect(rows[1].end).toBe("00:00:08.340");
    expect(await present(".statusbar__dirty")).toBe(true);
    // A command is not a save.
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("takes that back in one undo", async () => {
    await clickElement(toplevel, ".toolbar__edit-undo");
    await waitFor(async () => ((await gridRows())[1]?.start === "00:00:05.000" ? true : null), {
      timeout: 20000,
      message: "one undo to put the second row's start back",
    });
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("moves the video to the cursor's cue start, and to its end", async () => {
    await cursorTo(toplevel, 3);

    await runFromMenu(toplevel, "video-to-cue-start");
    const atStart = await waitFor(
      async () => {
        const now = asTimecode(await playhead());
        return now === THIRD_START ? now : null;
      },
      { timeout: 20000, message: `the player to reach ${THIRD_START}` },
    );
    expect(atStart).toBe(THIRD_START);

    await runFromMenu(toplevel, "video-to-cue-end");
    const atEnd = await waitFor(
      async () => {
        const now = asTimecode(await playhead());
        return now === THIRD_END ? now : null;
      },
      { timeout: 20000, message: `the player to reach ${THIRD_END}` },
    );
    expect(atEnd).toBe(THIRD_END);
  });

  it("puts the cursor on the cue that starts next when the video sits in a gap", async () => {
    await cursorTo(toplevel, 1);
    await seekTo(IN_THE_GAP);
    // Between the first cue's end and the second's start, so no cue covers this instant.
    expect(await playhead()).toBeGreaterThan(4.88);
    expect(await playhead()).toBeLessThan(5);

    await runFromMenu(toplevel, "edit-select-at-playhead");
    const rows = await waitFor(
      async () => {
        const now = await gridRows();
        return now[1]?.cursor === true ? now : null;
      },
      { timeout: 20000, message: "the cursor to land on the cue that starts next" },
    );
    // Forwards, not backwards: the cue that ended is not the one a translator is about to time.
    expect(rows.map((row) => row.cursor)).toEqual([false, true, false]);
  });
});
