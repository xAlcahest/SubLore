/* global describe, it, before, document, window */
/**
 * M2.7 E2 and E3: insert, delete, split and merge, driven from the Subtitles menu.
 *
 * The milestone's third criterion is about the route and not about the command, so nothing here
 * invokes a command: every edit is made by opening the menu and clicking the item, the way the user
 * has to. What is asserted afterwards is the grid, the one name that crossed the IPC boundary, the
 * bytes still on disk, and once the file read back after a save.
 *
 * The four run against one document in sequence, so the state each check leaves is the state the
 * next one opens with; the two at the end, which are about the cursor and the selection rather than
 * about the document, start from the file the save wrote and reopened.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { takeCommands, watchCommands } from "../lib/ipc.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** The fixture is committed and byte-frozen, so its text and its timings are facts, not readings. */
const OPEN_STATUS = "SRT · 3 cues · LF";
const FIRST = "The harbour was empty when we got there.";
const SECOND =
  "Nobody had told the crew we were coming,\nso we sat on the dock until it got light.";
const THIRD = "By then the fog had eaten the boats.";
/** The two halves the second cue divides into, at its own line break. */
const SECOND_HEAD = "Nobody had told the crew we were coming,";
const SECOND_TAIL = "so we sat on the dock until it got light.";
/** Where the caret goes before the split: the offset of that line break, counted in the box. */
const SPLIT_OFFSET = SECOND_HEAD.length;
/**
 * The cue's midpoint. No video is open in this check, so there is no playhead to divide at and the
 * midpoint is what the shell chooses (BACKLOG.md M2.7 E3).
 */
const SPLIT_AT = "00:00:06.670";
/** An inserted cue starts where the cursor's own cue ended and runs two seconds. */
const INSERTED_START = "00:00:04.880";
const INSERTED_END = "00:00:06.880";
/** The same pair as the SRT writer spells it, for reading the saved file back. */
const INSERTED_TIMING_LINE = "00:00:04,880 --> 00:00:06,880";

/** Writes go to the harness temp dir: the committed fixture is copied, never opened for editing. */
function workingCopy() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  if (!existsSync(source)) {
    throw new Error(
      `E2E prerequisite missing: ${source} does not exist. It is committed; restore it with ` +
        "`git checkout fixtures/subtitles`.",
    );
  }
  const directory = path.join(dataHome, "cue-structure");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(source, copy);
  return copy;
}

function key(name, repeat = 1) {
  execFileSync(
    "xdotool",
    ["key", "--clearmodifiers", "--repeat", String(repeat), "--repeat-delay", "10", name],
    { encoding: "utf8", timeout: 30000 },
  );
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

function clickCentre(toplevel, centre, what) {
  if (centre === null) {
    throw new Error(`${what} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

async function clickElement(toplevel, selector) {
  clickCentre(toplevel, await centreOf(selector), selector);
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

/** Every drawn row, in the order the grid draws them, with both states it carries. */
function gridRows() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__row")).map((row) => {
      const read = (css) => row.querySelector(css)?.textContent ?? null;
      return {
        text: read(".cuelist__text"),
        start: read(".cuelist__start"),
        end: read(".cuelist__end"),
        cursor: row.classList.contains("cuelist__row--active"),
        selected: row.getAttribute("aria-selected") === "true",
      };
    }),
  );
}

/** Wait until the grid holds exactly these texts, in this order, and hand the rows back. */
function waitForTexts(expected, what) {
  return waitFor(
    async () => {
      const rows = await gridRows();
      const same =
        rows.length === expected.length && rows.every((row, at) => row.text === expected[at]);
      return same ? rows : null;
    },
    { timeout: 20000, message: `the grid to hold ${what}` },
  );
}

/** The title of the open dropdown, or null when none is open. */
function openMenu() {
  return browser.execute(
    () => document.querySelector(".menubar__menu")?.getAttribute("aria-label") ?? null,
  );
}

/**
 * The only route these four have: open the Subtitles menu and click the item. The item is read for
 * its greying on the way past, so a command that ran while it was greyed cannot pass for a command
 * that ran because it was available.
 */
async function runFromMenu(toplevel, token) {
  await clickElement(toplevel, ".menubar__title--subtitle");
  await waitFor(async () => ((await openMenu()) === "Subtitles" ? true : null), {
    timeout: 15000,
    message: "the Subtitles dropdown to be the open one",
  });
  expect(await disabledOf(`#menuitem-${token}`)).toBe(false);
  await clickElement(toplevel, `#menuitem-${token}`);
  await waitFor(async () => ((await openMenu()) === null ? true : null), {
    timeout: 15000,
    message: "the dropdown to close behind the command it ran",
  });
}

/** Open a subtitle through the system chooser, which is the only route since T1. */
async function openSubtitle(toplevel, file) {
  await clickElement(toplevel, ".toolbar__file-open-subtitle");
  const chooser = await waitForChooser("Choose a subtitle");
  await answerChooser(chooser, file, "subtitle");
  focusWindow(toplevel.id);
  await waitFor(
    async () => ((await textOf(".statusbar__document"))?.startsWith("SRT") === true ? true : null),
    { timeout: 20000, message: "the status bar to report the subtitle that was opened" },
  );
}

/** Put the cursor on a row by clicking its number cell, which selects without opening an editor. */
async function cursorTo(toplevel, position) {
  clickCentre(toplevel, await centreOfCell(position, ".cuelist__pos"), `row ${position}`);
  return waitFor(
    async () => {
      const rows = await gridRows();
      return rows[position - 1]?.cursor === true ? rows : null;
    },
    { timeout: 15000, message: `the cursor to land on row ${position}` },
  );
}

function caretOffset() {
  return browser.execute(
    () => document.querySelector(".currentline__text")?.selectionStart ?? null,
  );
}

/**
 * Put the caret at a named offset in the current line's box: select everything, collapse left, then
 * walk right. A click lands wherever the glyphs happen to be, which is not an offset a check can
 * name, and the split divides the text at exactly one.
 */
async function placeCaret(toplevel, offset) {
  await clickElement(toplevel, ".currentline__text");
  await waitFor(
    () =>
      browser.execute(
        () => document.activeElement?.classList.contains("currentline__text") === true,
      ),
    { timeout: 15000, message: "the current-line box to take the keyboard" },
  );
  key("ctrl+a");
  key("Left");
  key("Right", offset);
  await waitFor(async () => ((await caretOffset()) === offset ? true : null), {
    timeout: 15000,
    message: `the caret to sit ${offset} characters into the box`,
  });
}

describe("the cue structure edits", () => {
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
    await waitFor(() => present(".toolbar__file-open-subtitle"), {
      timeout: 30000,
      message: "the app UI to render",
    });
    await openSubtitle(toplevel, copy);
    await waitForTexts([FIRST, SECOND, THIRD], "the three cues the fixture holds");
  });

  it("inserts a cue under the cursor, from the menu", async () => {
    // A document opens on its first row (decision 5), which is the row the insert goes under.
    const before = await gridRows();
    expect(before[0].cursor).toBe(true);

    await watchCommands();
    await runFromMenu(toplevel, "subtitle-insert");

    const after = await waitForTexts(
      [FIRST, "", SECOND, THIRD],
      "an empty cue between the first and the second",
    );
    // One command, and the one the menu item names. A route that ran twice, or that ran something
    // else on the way, shows up here as a second name rather than as a document that looks right.
    expect(await takeCommands()).toEqual(["subtitle_insert"]);
    expect(after[1].start).toBe(INSERTED_START);
    expect(after[1].end).toBe(INSERTED_END);
    // The rows around it keep their own timings: an insert is not a re-timing.
    expect(after[0].end).toBe(INSERTED_START);
    expect(after[2].start).toBe("00:00:05.000");
    expect(await textOf(".statusbar__document")).toContain("4 cues");
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await present(".statusbar__error")).toBe(false);
    // An edit is not a save, exactly as it is not one for the text.
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("deletes the cue the cursor is on, from the menu", async () => {
    await cursorTo(toplevel, 4);

    await watchCommands();
    await runFromMenu(toplevel, "subtitle-delete");

    await waitForTexts([FIRST, "", SECOND], "the last cue gone");
    expect(await takeCommands()).toEqual(["subtitle_delete"]);
    expect(await textOf(".statusbar__document")).toContain("3 cues");
    expect(await present(".statusbar__error")).toBe(false);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("splits a cue at the caret in the current line, from the menu", async () => {
    await cursorTo(toplevel, 3);
    await placeCaret(toplevel, SPLIT_OFFSET);

    await watchCommands();
    await runFromMenu(toplevel, "subtitle-split");

    const after = await waitForTexts(
      [FIRST, "", SECOND_HEAD, SECOND_TAIL],
      "the second cue divided at the caret",
    );
    expect(await takeCommands()).toEqual(["subtitle_split"]);
    // The text divided at the caret and the times at the millisecond the shell chose.
    expect(after[2].start).toBe("00:00:05.000");
    expect(after[2].end).toBe(SPLIT_AT);
    expect(after[3].start).toBe(SPLIT_AT);
    expect(after[3].end).toBe("00:00:08.340");
    expect(await present(".statusbar__error")).toBe(false);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("merges the cursor's cue with the one after it, from the menu", async () => {
    // The split left the cursor where it was, on the first of the two halves.
    expect((await gridRows())[2].cursor).toBe(true);

    await watchCommands();
    await runFromMenu(toplevel, "subtitle-merge");

    const after = await waitForTexts([FIRST, "", SECOND], "the two halves joined again");
    expect(await takeCommands()).toEqual(["subtitle_merge"]);
    // The join takes the first cue's start and the second's end, so the pair spans what they did.
    expect(after[2].start).toBe("00:00:05.000");
    expect(after[2].end).toBe("00:00:08.340");
    expect(await present(".statusbar__error")).toBe(false);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("takes the four back one undo each, and puts them back one redo each", async () => {
    // The same undo the text edits use, which is the criterion: the toolbar's own button, not a
    // second stack of the structure edits' own.
    const undo = () => clickElement(toplevel, ".toolbar__edit-undo");
    const redo = () => clickElement(toplevel, ".toolbar__edit-redo");

    await undo();
    await waitForTexts([FIRST, "", SECOND_HEAD, SECOND_TAIL], "the merge undone");
    await undo();
    await waitForTexts([FIRST, "", SECOND], "the split undone");
    await undo();
    await waitForTexts([FIRST, "", SECOND, THIRD], "the delete undone");
    await undo();
    await waitForTexts([FIRST, SECOND, THIRD], "the insert undone");

    // Four edits, four steps, and the document is the one that was opened: the unsaved marker
    // clearing is what says the stack came all the way back rather than most of the way.
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the unsaved marker to clear once all four are undone",
    });
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);

    await redo();
    await waitForTexts([FIRST, "", SECOND, THIRD], "the insert redone");
    await redo();
    await waitForTexts([FIRST, "", SECOND], "the delete redone");
    await redo();
    await waitForTexts([FIRST, "", SECOND_HEAD, SECOND_TAIL], "the split redone");
    await redo();
    await waitForTexts([FIRST, "", SECOND], "the merge redone");
    expect(await present(".statusbar__dirty")).toBe(true);
  });

  it("writes nothing until a save, and the saved file reopens as what the grid showed", async () => {
    // Everything above ran against a file that never changed. This is the line that changes it.
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
    await clickElement(toplevel, ".toolbar__file-save");
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the unsaved marker to clear once the document is written",
    });

    const written = readFileSync(copy).toString("utf8");
    expect(written).toContain(INSERTED_TIMING_LINE);
    expect(written).toContain(SECOND);
    // The deleted cue is gone from the file, not merely from the grid.
    expect(written).not.toContain(THIRD);

    // Round trip: the file is read back through the chooser and the grid draws what was saved.
    await openSubtitle(toplevel, copy);
    const reopened = await waitForTexts([FIRST, "", SECOND], "the saved document read back");
    // The barrier the round trip needs: the grid held these three rows before the chooser was
    // answered too, and only a document that was really re-read puts the cursor back on row one.
    expect(reopened.map((row) => row.cursor)).toEqual([true, false, false]);
    expect(reopened.map(({ start, end }) => ({ start, end }))).toEqual([
      { start: "00:00:02.120", end: INSERTED_START },
      { start: INSERTED_START, end: INSERTED_END },
      { start: "00:00:05.000", end: "00:00:08.340" },
    ]);
    expect(await textOf(".statusbar__document")).toContain(OPEN_STATUS);
    expect(await present(".statusbar__error")).toBe(false);
  });

  it("keeps the cursor and the selection on the lines they were on when rows move", async () => {
    // A range across all three rows, built the way the grid builds one: a click, then two extends.
    await cursorTo(toplevel, 1);
    key("shift+Down", 2);
    const ranged = await waitFor(
      async () => {
        const rows = await gridRows();
        return rows[2]?.cursor === true ? rows : null;
      },
      { timeout: 15000, message: "the cursor to reach the third row with the range behind it" },
    );
    expect(ranged.map((row) => row.selected)).toEqual([true, true, true]);

    // The new cue goes in under the cursor, so nothing either state points at has moved.
    await runFromMenu(toplevel, "subtitle-insert");
    const grown = await waitForTexts([FIRST, "", SECOND, ""], "the inserted cue below the cursor");
    expect(grown.map((row) => row.cursor)).toEqual([false, false, true, false]);
    expect(grown.map((row) => row.selected)).toEqual([true, true, true, false]);

    // Now a row goes from under them. The cursor stays on the row that took its place, and the
    // selection comes up with the rows rather than swallowing the one that moved into the gap.
    await runFromMenu(toplevel, "subtitle-delete");
    const shrunk = await waitForTexts([FIRST, "", ""], "the cursor's cue gone");
    expect(shrunk.map((row) => row.cursor)).toEqual([false, false, true]);
    expect(shrunk.map((row) => row.selected)).toEqual([true, true, false]);
  });

  it("never leaves the cursor past the end when the last cue is the one deleted", async () => {
    await cursorTo(toplevel, 3);

    await runFromMenu(toplevel, "subtitle-delete");

    const left = await waitForTexts([FIRST, ""], "the last cue gone");
    // Nothing below to fall onto, so it clamps to the new last row. Past the end draws no cursor
    // at all and leaves the current line saying it has no row, which is the shape of the defect.
    expect(left.map((row) => row.cursor)).toEqual([false, true]);
    // The selection never empties while rows stand, so it comes down onto the same row.
    expect(left.map((row) => row.selected)).toEqual([false, true]);
    expect(await present(".currentline__empty")).toBe(false);
  });
});
