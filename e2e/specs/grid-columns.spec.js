/* global describe, it, before, after, document, window */
/**
 * The Style and Actor columns (docs/grid-columns-tasks.md, G3).
 *
 * Two columns the grid draws for an ASS file, read out of the event's own `Format:` line by name
 * rather than by position, and absent altogether when no cue in the list fills them. Nothing here
 * reads a source file to decide what the grid should show: every expectation below is either a fact
 * of a byte-frozen fixture or one grid compared against another.
 *
 * Two of the checks are about the shell rather than about the grid. The floor one: the window's
 * smallest width is measured off `.cuelist__head` among other rows and re-measured when the
 * interface size changes, so a size picked while a document is already open is what makes the
 * reading that document's. The alignment one: the head is a sibling of the scrolling list and not a
 * row inside it, so the two are separate flex lines, and cells that take the row's slack only land
 * on the same boundaries while both lines have the same width to divide.
 */
import { Buffer } from "node:buffer";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clippedAtWindowEdge } from "../lib/clipping.js";
import { askForWindowSize, clickAt, dragAt, focusWindow, waitForWindowSize } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { interfaceScale } from "../lib/scale.js";
import { findToplevel, rootTree, windowSize } from "../lib/x11.js";

/** Percentage widths and scaled type both land on fractions of a pixel. */
const SLOP_PX = 1;

/** The size the app is left at, which is the size a launch with nothing stored opens at (S1). */
const DEFAULT_PERCENT = 110;

/** The edge between the top block and the grid: dragged down, the grid keeps fewer rows. */
const GRID_SASH = ".sash--grid";

/** The head as an SRT draws it, in the order it draws it: the grid this change must not alter. */
const SRT_HEAD = ["#", "No.", "Start", "End", "CPS", "Text"];
/** The same head with the two new columns, which go between CPS and Text. */
const ASS_HEAD = ["#", "No.", "Start", "End", "CPS", "Style", "Actor", "Text"];
/** And with the speaker column absent, which is what a file naming nobody draws. */
const STYLE_ONLY_HEAD = ["#", "No.", "Start", "End", "CPS", "Style", "Text"];

/**
 * The row cell each head cell stands over, in the order both draw them. Paired by position, which
 * is the only thing that makes a head cell the label of a column rather than a word near one.
 */
const ROW_CELLS = [
  ".cuelist__pos",
  ".cuelist__number",
  ".cuelist__start",
  ".cuelist__end",
  ".cuelist__cps",
  ".cuelist__style",
  ".cuelist__actor",
  ".cuelist__text",
];

/**
 * `speakers.ass`, by hand, because it is committed and byte-frozen: its five events, their times as
 * the grid spells them, the style each names and the speaker on the three that have one.
 */
const SPEAKERS = [
  {
    start: "00:00:01.340",
    end: "00:00:03.980",
    style: "Default",
    actor: "Ingrid",
    text: "The harbour freezes over by December.",
  },
  {
    start: "00:00:04.120",
    end: "00:00:06.750",
    style: "Default",
    actor: "Marek",
    text: "Then we sail in November, like everyone else.",
  },
  {
    start: "00:00:07.000",
    end: "00:00:09.440",
    style: "Sign",
    actor: "",
    text: "HARBOUR OFFICE - CLOSED",
  },
  {
    start: "00:00:09.600",
    end: "00:00:12.100",
    style: "Default",
    actor: "Ingrid",
    text: "Everyone else lost a boat last year.",
  },
  {
    start: "00:00:12.300",
    end: "00:00:14.900",
    style: "Default",
    actor: "",
    text: "Nobody signed for the delivery.",
  },
];

/** `non-latin.ass`: two styles, one of them not Latin, and an empty name on all four events. */
const NON_LATIN_STYLES = ["見出し", "Default", "Default", "Default"];

/** `basic.ass`: three events, one style throughout, and one line the file gives a speaker. */
const BASIC_ACTORS = ["", "Ingrid", ""];
const BASIC_TEXTS = [
  "The harbour freezes over by December.",
  "Then we sail in November, like everyone else.",
  "Everyone else lost a boat last year.",
];
/** The row the Ingrid line sits on, 1-based, and what an insert under it inherits. */
const INGRID_POSITION = 2;

/** Subtitle fixtures are committed: a missing one is a broken checkout, not a skipped check. */
function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with ` +
        "`git checkout fixtures/subtitles`.",
    );
  }
  return file;
}

const SPEAKERS_FILE = () => fixture("ass", "clean", "speakers.ass");
const SHUFFLED_FILE = () => fixture("ass", "clean", "speakers-shuffled.ass");
const SPELLING_FILE = () => fixture("ass", "clean", "actor-spelling.ass");
const MINIMAL_FILE = () => fixture("ass", "clean", "minimal-fields.ass");
const NON_LATIN_FILE = () => fixture("ass", "clean", "non-latin.ass");
const BASIC_FILE = () => fixture("ass", "clean", "basic.ass");
const SRT_FILE = () => fixture("srt", "clean", "basic-lf.srt");

/** Writes go to the harness temp dir, never into the repo and never beside a fixture. */
function saveDirectory() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "grid-columns");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
}

const storedLayout = () =>
  path.join(process.env.SUBLORE_E2E_DATA_HOME, "com.sublore.app", "layout.json");

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { midX: (rect.x + rect.width / 2) * dpr, midY: (rect.y + rect.height / 2) * dpr };
  }, selector);
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  if (rect === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + rect.midX, toplevel.absY + rect.midY);
}

/** Drag one edge and wait for the release to settle, the way `dividers.spec.js` drags one. */
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
  await browser.pause(250);
}

/** Click one cell of the row at a 1-based list position, if that row is rendered. */
async function clickCell(toplevel, position, cell) {
  const centre = await browser.execute(
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
  if (centre === null) {
    throw new Error(`row ${position} has no ${cell} in the DOM`);
  }
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

/** Every head cell, in the order the head draws them, with the class list each carries. */
function headCells() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__head > *")).map((cell) => ({
      label: cell.textContent,
      classes: cell.className,
    })),
  );
}

/** Just the labels, which is what a column being drawn or not looks like from outside. */
async function headLabels() {
  return (await headCells()).map((cell) => cell.label);
}

/**
 * Every drawn row, cell by cell. A cell that is not in the row at all reads as null, which is how a
 * column that takes no width is told apart from one drawn empty.
 */
function gridRows() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__row")).map((row) => {
      const read = (css) => {
        const cell = row.querySelector(css);
        return cell === null ? null : cell.textContent;
      };
      return {
        position: read(".cuelist__pos"),
        number: read(".cuelist__number"),
        start: read(".cuelist__start"),
        end: read(".cuelist__end"),
        cps: read(".cuelist__cps"),
        style: read(".cuelist__style"),
        actor: read(".cuelist__actor"),
        text: read(".cuelist__text"),
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

/** Wait until the head draws exactly these labels, and hand the rows under it back. */
async function waitForHead(labels, what) {
  await waitFor(
    async () => {
      const drawn = await headLabels();
      return drawn.length === labels.length && drawn.every((label, at) => label === labels[at])
        ? drawn
        : null;
    },
    { timeout: 20000, message: `the grid head to draw ${what}` },
  );
  return gridRows();
}

/**
 * Open a subtitle through the system chooser, which is the only route since T1.
 *
 * The wait is for a list that is not the one that was up, not for a row: two of the checks below
 * open files that draw the same number of rows and the same head, and a wait on either would be
 * satisfied by the document that was already there. `CueList` is keyed on a counter the shell bumps
 * per open (`src/App.tsx:1266`, `src/hooks/useSubtitleFile.ts:207`), so the marked element is gone
 * exactly when the new document has been drawn.
 */
async function openSubtitle(toplevel, file) {
  await browser.execute(() => {
    const list = document.querySelector(".cuelist__panel");
    if (list !== null) {
      list.dataset.beforeOpen = "";
    }
  });
  await clickElement(toplevel, ".toolbar__file-open-subtitle");
  const chooser = await waitForChooser("Choose a subtitle");
  await answerChooser(chooser, file, "subtitle");
  focusWindow(toplevel.id);
  await waitFor(
    () =>
      browser.execute(() => {
        const list = document.querySelector(".cuelist__panel");
        return (
          list !== null &&
          list.dataset.beforeOpen === undefined &&
          list.querySelector(".cuelist__row") !== null
        );
      }),
    { timeout: 20000, message: `the grid to be redrawn from ${path.basename(file)}` },
  );
}

/** The title of the open dropdown, or null when none is open. */
function openMenu() {
  return browser.execute(
    () => document.querySelector(".menubar__menu")?.getAttribute("aria-label") ?? null,
  );
}

/** Open the Subtitles menu and click an item, reading its greying on the way past. */
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

/** Put the cursor on a row by clicking its number cell, which selects without opening an editor. */
async function cursorTo(toplevel, position) {
  await clickCell(toplevel, position, ".cuelist__pos");
  return waitFor(
    async () => {
      const rows = await browser.execute(() =>
        Array.from(document.querySelectorAll(".cuelist__row")).map((row) =>
          row.classList.contains("cuelist__row--active"),
        ),
      );
      return rows[position - 1] === true ? rows : null;
    },
    { timeout: 15000, message: `the cursor to land on row ${position}` },
  );
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
    {
      timeout: 5000,
      message: `the interface to be drawn at ${percent} per cent`,
    },
  );
  // Picking is also what stores the size, and the floor is measured in the same pass.
  await browser.pause(250);
}

/**
 * The narrowest the shell says the window may be. There is no number here to check it against: it
 * is measured off the rows that cannot be drawn narrower than what is in them, and every width in
 * those is a width the machine's fonts decide.
 */
async function derivedFloor() {
  const said = await browser.execute(
    () => document.querySelector(".shell")?.dataset.minimumWidth ?? null,
  );
  const floor = Number(said);
  if (!Number.isInteger(floor) || floor <= 0) {
    throw new Error(
      `the shell says its smallest width is ${JSON.stringify(said)}, which is not a width. ` +
        "Nothing measured a floor, so there is nothing the window could have been held at.",
    );
  }
  return floor;
}

/**
 * Ask X for one width and wait for the window to settle at another. The two are the same for every
 * width the shell can be drawn at; a request under the shell's own floor is the case they differ
 * in, and the window coming back to the floor is what a smallest window width means.
 */
async function settleAt(id, ask, height, took) {
  askForWindowSize(id, ask, height);
  waitForWindowSize(id, took, height);
  await waitFor(
    async () => ((await browser.execute(() => window.innerWidth)) === took ? 1 : null),
    {
      timeout: 15000,
      message: `the page to be laid out ${took} CSS pixels wide`,
    },
  );
  const toplevel = findToplevel({ width: took, height });
  if (toplevel === null) {
    throw new Error(`no ${took}x${height} "Sublore" toplevel after the resize.\n${rootTree()}`);
  }
  return toplevel;
}

/**
 * Each head cell's left edge beside the left edge of the row cell it stands over, and whether the
 * list is scrolling under it. The head is outside the scrolling box, so a scrollbar that takes
 * width is width the rows divide and the head does not.
 */
function columnEdges() {
  return browser.execute((cells) => {
    const list = document.querySelector(".cuelist");
    const row = document.querySelector(".cuelist__row");
    const heads = Array.from(document.querySelectorAll(".cuelist__head > *"));
    if (list === null || row === null) {
      return null;
    }
    return {
      scrolling: list.scrollHeight > list.clientHeight,
      columns: heads.map((head, at) => {
        const cell = at < cells.length ? row.querySelector(cells[at]) : null;
        return {
          label: head.textContent,
          head: Math.round(head.getBoundingClientRect().left * 100) / 100,
          row: cell === null ? null : Math.round(cell.getBoundingClientRect().left * 100) / 100,
        };
      }),
    };
  }, ROW_CELLS);
}

/** Every column whose label does not sit over the cells it names, by more than a pixel. */
function misaligned(reading) {
  if (reading === null) {
    throw new Error("the grid drew no head or no row, so there was nothing to line up");
  }
  return reading.columns
    .filter((column) => column.row === null || Math.abs(column.head - column.row) > SLOP_PX)
    .map((column) => `${column.label}: head at ${column.head}, column at ${column.row}`);
}

describe("the grid's style and actor columns", () => {
  let toplevel = null;
  let saveDir = null;

  before(async () => {
    saveDir = saveDirectory();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => present(".toolbar__file-open-subtitle"), {
      timeout: 30000,
      message: "the app UI to render",
    });
  });

  // The interface size and the sash positions live in one store shared with every other spec, and
  // the two shell checks pick sizes and drag an edge. This leaves the store as
  // `interface-scale.spec.js` does.
  after(() => {
    rmSync(storedLayout(), { force: true });
  });

  it("draws both columns for a file whose events name a style and a speaker", async () => {
    await openSubtitle(toplevel, SPEAKERS_FILE());

    const rows = await waitForHead(ASS_HEAD, "the six it draws for an SRT plus Style and Actor");
    expect(rows.map(({ style, actor, text }) => ({ style, actor, text }))).toEqual(
      SPEAKERS.map(({ style, actor, text }) => ({ style, actor, text })),
    );
    // Read off the same rows: a column drawn beside the wrong line would still hold the right set.
    expect(rows.map(({ start, end }) => ({ start, end }))).toEqual(
      SPEAKERS.map(({ start, end }) => ({ start, end })),
    );
  });

  it("reads both by name, so a shuffled Format line draws the same rows", async () => {
    await openSubtitle(toplevel, SPEAKERS_FILE());
    const canonical = await waitForHead(ASS_HEAD, "both columns for the canonical field order");

    // The same cues, with Name declared first and Style last before Text. This is the check that
    // fails if either index is ever hard-coded to a position.
    await openSubtitle(toplevel, SHUFFLED_FILE());
    const shuffled = await waitForHead(ASS_HEAD, "both columns for the shuffled field order");

    expect(shuffled).toEqual(canonical);
  });

  it("reads the speaker field when the file spells it Actor", async () => {
    await openSubtitle(toplevel, SPELLING_FILE());

    const rows = await waitForHead(ASS_HEAD, "both columns for a file declaring Actor");
    expect(rows.map((row) => row.actor)).toEqual(SPEAKERS.map((cue) => cue.actor));
  });

  it("draws no actor column for a file where no event names anyone", async () => {
    await openSubtitle(toplevel, NON_LATIN_FILE());

    const rows = await waitForHead(STYLE_ONLY_HEAD, "a Style column and no Actor column");
    expect(rows.map((row) => row.style)).toEqual(NON_LATIN_STYLES);
    // Not drawn empty: the cell is out of every row, which is what taking no width means.
    expect(rows.map((row) => row.actor)).toEqual(NON_LATIN_STYLES.map(() => null));
  });

  it("draws neither column for a file whose Format line declares neither", async () => {
    await openSubtitle(toplevel, MINIMAL_FILE());

    const rows = await waitForHead(SRT_HEAD, "the six cells it draws for an SRT");
    expect(rows.map(({ style, actor }) => ({ style, actor }))).toEqual(
      rows.map(() => ({ style: null, actor: null })),
    );
  });

  it("leaves an SRT the grid it was", async () => {
    await openSubtitle(toplevel, SRT_FILE());

    const cells = await waitFor(
      async () => {
        const drawn = await headCells();
        return drawn.length === SRT_HEAD.length ? drawn : null;
      },
      { timeout: 20000, message: "the grid head to come back to six cells" },
    );
    expect(cells.map((cell) => cell.label)).toEqual(SRT_HEAD);
    // No cell carries either class, head or row: an SRT has no style and no speaker to draw.
    expect(cells.filter((cell) => /style|actor/.test(cell.classes))).toEqual([]);
    const rows = await gridRows();
    expect(rows.map(({ style, actor }) => ({ style, actor }))).toEqual(
      rows.map(() => ({ style: null, actor: null })),
    );
  });

  it("does not move the window's floor, at any of the three interface sizes", async () => {
    /**
     * The floor is re-measured when the interface size changes, so a size picked twice with a
     * document already in the grid is what makes the reading that document's. Read straight after
     * opening a file it would be the reading taken for whatever was open before.
     */
    const floorWith = async (file, percent) => {
      await openSubtitle(toplevel, file);
      await pickSize(toplevel, percent === 100 ? 90 : 100);
      await pickSize(toplevel, percent);
      return derivedFloor();
    };

    for (const percent of [90, 110, 150]) {
      const withSrt = await floorWith(SRT_FILE(), percent);
      const withAss = await floorWith(SPEAKERS_FILE(), percent);
      expect({ percent, withAss }).toEqual({ percent, withAss: withSrt });
    }

    // And the narrowest window there is at the largest size, with the wider head open in it: the
    // head is not cut off at the window edge and the page has not started scrolling sideways.
    const floor = await derivedFloor();
    toplevel = await settleAt(toplevel.id, floor - 1, windowHeight, floor);
    expect(windowSize(toplevel.id)?.width ?? null).toBe(floor);
    expect(await clippedAtWindowEdge(SLOP_PX)).toEqual([]);
    const across = await browser.execute(() => ({
      document: document.documentElement.scrollWidth,
      body: document.body.scrollWidth,
      client: document.documentElement.clientWidth,
    }));
    expect(across.document > across.client || across.body > across.client).toBe(false);

    // Back to the size and the window the checks below expect. The size goes first: the floor at
    // 150 per cent is wider than the window the run opens at, so the width comes from the floor.
    await pickSize(toplevel, DEFAULT_PERCENT);
    const back = Math.max(windowWidth, await derivedFloor());
    toplevel = await settleAt(toplevel.id, back, windowHeight, back);
  });

  it("takes the actor column away with the only cue that named anyone, and undo brings it back", async () => {
    await openSubtitle(toplevel, BASIC_FILE());
    const opened = await waitForHead(ASS_HEAD, "both columns for the three-event fixture");
    expect(opened.map((row) => row.actor)).toEqual(BASIC_ACTORS);

    await cursorTo(toplevel, INGRID_POSITION);
    await runFromMenu(toplevel, "subtitle-delete");

    const left = await waitForHead(STYLE_ONLY_HEAD, "the Actor column gone with its only cue");
    expect(left.map((row) => row.text)).toEqual([BASIC_TEXTS[0], BASIC_TEXTS[2]]);
    expect(left.map((row) => row.actor)).toEqual([null, null]);

    await clickElement(toplevel, ".toolbar__edit-undo");

    const back = await waitForHead(ASS_HEAD, "the Actor column back with the cue that filled it");
    expect(back.map((row) => row.actor)).toEqual(BASIC_ACTORS);
    // One step, and the document is the one that was opened: nothing here ever wrote the file.
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the unsaved marker to clear once the delete is undone",
    });
  });

  it("gives an inserted cue the style and the speaker of the line it was copied from", async () => {
    await cursorTo(toplevel, INGRID_POSITION);

    await runFromMenu(toplevel, "subtitle-insert");

    const rows = await waitForTexts(
      [BASIC_TEXTS[0], BASIC_TEXTS[1], "", BASIC_TEXTS[2]],
      "an empty cue under the row that names Ingrid",
    );
    // No code was written for this: an inserted ASS event mirrors its neighbour's whole line, so
    // the two columns come with it. The check exists so that stays true.
    expect(rows[2].style).toBe(rows[1].style);
    expect(rows[2].actor).toBe(rows[1].actor);
    expect(rows[2].actor).toBe(BASIC_ACTORS[INGRID_POSITION - 1]);

    // Back to the document that was opened, so nothing below starts from an edited one.
    await clickElement(toplevel, ".toolbar__edit-undo");
    await waitForTexts(BASIC_TEXTS, "the insert undone");
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the unsaved marker to clear once the insert is undone",
    });
  });

  it("writes the bytes it opened, with both columns drawn", async () => {
    const source = SPEAKERS_FILE();
    await openSubtitle(toplevel, source);
    await waitForHead(ASS_HEAD, "both columns before the save");

    const destination = path.join(saveDir, path.basename(source));
    await clickElement(toplevel, ".toolbar__file-save-copy");
    const chooser = await waitForChooser("Save a copy of the subtitle");
    await answerChooser(chooser, destination, "save a copy");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__message"))?.includes(destination) === true,
      {
        timeout: 20000,
        message: `the status line to report the copy at ${destination}`,
      },
    );

    expect(await textOf(".statusbar__error")).toBe(null);
    expect(Buffer.compare(readFileSync(source), readFileSync(destination))).toBe(0);
  });

  it("keeps every head cell over its own column, list scrolling or not", async () => {
    await openSubtitle(toplevel, SPEAKERS_FILE());
    await waitForHead(ASS_HEAD, "both columns before the edge is dragged");

    // Three cells take the row's slack now, so a head and a row with different widths to divide put
    // two of the labels somewhere other than over their columns. Still first, then scrolling: the
    // head is not inside the box a scrollbar would take its width from.
    const still = await columnEdges();
    expect({ scrolling: still?.scrolling, off: misaligned(still) }).toEqual({
      scrolling: false,
      off: [],
    });

    // The block takes everything the grid may give up, which leaves the grid its floor of a head
    // and three rows: five cues then need a scrollbar, and the drag is the only way to ask for one.
    await dragSash(toplevel, GRID_SASH, 0, 2000);
    const scrolled = await columnEdges();
    expect({ scrolling: scrolled?.scrolling, off: misaligned(scrolled) }).toEqual({
      scrolling: true,
      off: [],
    });

    await dragSash(toplevel, GRID_SASH, 0, -2000);
  });
});
