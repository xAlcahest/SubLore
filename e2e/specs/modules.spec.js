/* global describe, it, before, document, window */
/**
 * Criteria 2, 3 and 4 of `module-abi.md` §9, through the app: a module file that is present and does
 * not load is a fault, and a fault is said out loud, once transiently in the status bar and once
 * permanently in About.
 *
 * The fixture is put beside the executable by `wdio.conf.js`, not here. The app is launched by the
 * session, before any hook in this file runs, so a fixture installed in a `before` would arrive
 * after the scan it is meant to be found by.
 *
 * Criterion 1, that an absent module is silence, is the other twenty-six spec files: every one of
 * them runs with no module beside the executable and none of them has ever drawn a module line.
 * Asserting it once more here would be asserting it about the same state they all establish.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";

import { browser, expect } from "@wdio/globals";

import { appLog, dataHome } from "../lib/applog.js";
import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import {
  chooseRailItem,
  closeAnyOpenProject,
  confirmRailDialog,
  openProjectMenu,
} from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

/** What the refused fixture beside the executable is, and what this build speaks. */
const FIXTURE_FILE = "sublore_module_wrong_major.so";
/** The good fixture's own title, rendered in the locale the host handed it. */
const MODULE_TITLE = "Fixture (en)";
/**
 * The fixture's item, with the answer the host gave it in the label.
 *
 * The fixture asks the host to find "the fog" in "the {\\i1}fog and the fog" with tags skipped.
 * Those bytes do not contain the term and what a reader sees does, so a two says the comparison
 * came from `sublore-matcher` through the host table and not from anything in the module.
 */
const MODULE_ITEM = "Rewrite the first line (2 found)";
/** The item that proposes against a revision the session has moved past (module-abi.md 9.6). */
const STALE_ITEM = "Rewrite from a stale revision";
/** The item that writes into the module's own table, which needs a project to write into. */
const STORE_ITEM = "Store a note";
/**
 * What the module says after it stored one, with the number of rows its own table then held.
 *
 * The count is the whole evidence. A row that survives the project being closed and opened again is
 * a row that reached the file, which nothing this check can see from the outside would otherwise
 * say. See `module-abi.md` §4.7 and criterion 7 of §9.
 */
const STORED = /module sublore_module_fixture\.so: stored a note, and the table now holds (\d+)/g;
/** What the fixture puts in the first cue, in its own words so nothing else could have written it. */
const WROTE = "The module wrote this line.";
/** The first cue of the fixture below, before any module touches it. */
const FIRST = "The harbour was empty when we got there.";
/** The fixture reports one above the host's major, and the host's is 1. */
const THEIRS = 2;
const OURS = 1;

/** Writes go to the harness temp dir: the committed fixture is copied, never opened for editing. */
function workingCopy() {
  const source = path.join(repoRoot, "fixtures", "subtitles", "srt", "clean", "basic-lf.srt");
  if (!existsSync(source)) {
    throw new Error(
      `E2E prerequisite missing: ${source} does not exist. It is committed; restore it with ` +
        "`git checkout fixtures/subtitles`.",
    );
  }
  const directory = path.join(dataHome(), "modules");
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
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

/** The text of every row the grid draws. */
function gridTexts() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".cuelist__row")).map(
      (row) => row.querySelector(".cuelist__text")?.textContent ?? "",
    ),
  );
}

/** Open the module's own dropdown and click one of its items by the label it drew. */
async function runModuleItem(toplevel, label) {
  await clickElement(toplevel, ".menubar__title--module-0-1");
  await waitFor(() => present(".menubar__menu"), {
    timeout: 15000,
    message: "the module's own dropdown to open",
  });
  const found = await browser.execute(
    (want) =>
      Array.from(document.querySelectorAll(".menubar__item")).find((item) =>
        (item.textContent ?? "").includes(want),
      )?.id ?? null,
    label,
  );
  if (found === null) {
    throw new Error(`the module drew no item reading ${label}`);
  }
  await clickElement(toplevel, `#${found}`);
  await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
    timeout: 15000,
    message: "the dropdown to close behind the item it ran",
  });
}

/** Whether the module's storage item is on the menu, and whether it can be run. */
async function storeItemState(toplevel) {
  await clickElement(toplevel, ".menubar__title--module-0-1");
  await waitFor(() => present(".menubar__menu"), {
    timeout: 15000,
    message: "the module's own dropdown to open",
  });
  const state = await browser.execute((want) => {
    const item = Array.from(document.querySelectorAll(".menubar__item")).find((each) =>
      (each.textContent ?? "").includes(want),
    );
    return item === undefined
      ? { drawn: false, enabled: false }
      : { drawn: true, enabled: !item.disabled };
  }, STORE_ITEM);
  pressKey("Escape");
  await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
    timeout: 15000,
    message: "the dropdown to close again",
  });
  return state;
}

/** Create a project in `folder` through the rail, the way a person reaches it. */
async function makeProjectIn(toplevel, folder) {
  await openProjectMenu(toplevel);
  await chooseRailItem(toplevel, "create-project");
  const chooser = await waitForChooser("Choose a project folder");
  await answerChooser(chooser, folder, "project folder");
  focusWindow(toplevel.id);
  await waitFor(() => present(".rail__project"), {
    timeout: 20000,
    message: "the project to reach the rail after it was created",
  });
}

/**
 * Run the module's storage item once and answer the row count it reported.
 *
 * Counted from a mark taken before the gesture, so a line left by an earlier run of the same item
 * cannot stand in for this one.
 */
async function storeAndRead(toplevel) {
  const before = (appLog(dataHome()).match(STORED) ?? []).length;
  await runModuleItem(toplevel, STORE_ITEM);
  const said = await waitFor(
    () => {
      const all = [...appLog(dataHome()).matchAll(STORED)];
      return all.length > before ? all[all.length - 1] : null;
    },
    { timeout: 20000, message: "the module to say what it stored" },
  );
  return Number(said[1]);
}

describe("modules beside the executable", () => {
  let toplevel = null;

  before(async () => {
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__file-open-subtitle") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
  });

  it("puts the module's own title on the menu bar, with its item under it", async () => {
    // The core never learned this word. It came out of the module through describe, in the locale
    // the host handed it, and the title exists because a module pushed one (module-abi.md 5.1).
    const titles = await browser.execute(() =>
      Array.from(document.querySelectorAll(".menubar__title")).map(
        (title) => title.textContent ?? "",
      ),
    );
    expect(titles).toContain(MODULE_TITLE);

    const bar = await findToplevel();
    await clickElement(bar, `.menubar__title--module-0-1`);
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the module's own dropdown to open",
    });
    const items = await browser.execute(() =>
      Array.from(document.querySelectorAll(".menubar__item")).map((item) => item.textContent ?? ""),
    );
    expect(items.some((item) => item.includes(MODULE_ITEM))).toBe(true);
    expect(items.some((item) => item.includes(STORE_ITEM))).toBe(true);
    // Exactly three. The fixture pushes a fourth item with a state this build does not know, and
    // section 5.2 says that costs the module its item rather than giving the user a control that
    // is enabled when it should not be.
    expect(items).toHaveLength(3);
    expect(items.some((item) => item.includes(STALE_ITEM))).toBe(true);

    pressKey("Escape");
    await waitFor(async () => ((await present(".menubar__menu")) === false ? true : null), {
      timeout: 15000,
      message: "the dropdown to close",
    });
  });

  it("puts what the module logged in the app's own log, under the module's file name", () => {
    // Written by `describe`, through the host's own logger, and carrying the file it came from so
    // the line can be told from a core one (module-abi.md 4.2).
    expect(appLog(dataHome())).toContain(
      'module sublore_module_fixture.so: asked for "the fog" and was given 2 of them',
    );
  });

  it("says so in the status bar, naming the file and both versions", async () => {
    const line = await waitFor(async () => (await textOf(".statusbar__module-error")) ?? null, {
      timeout: 20000,
      message: "the status bar to report the module it could not use",
    });

    // The file name comes from the directory and the versions are integers, so the sentence is a
    // core string with data in it and the core never learned what the module was for.
    expect(line).toContain(FIXTURE_FILE);
    expect(line).toContain(String(THEIRS));
    expect(line).toContain(String(OURS));
  });

  it("changes a cue when its own item is activated, and one undo takes it back", async () => {
    // Criterion 5 of module-abi.md section 9. The module names a cue and the text; the host runs
    // the same three guards every interactive edit runs and makes the edit itself.
    const copy = workingCopy();
    const openedBytes = readFileSync(copy);

    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const chooser = await waitForChooser("Choose a subtitle");
    await answerChooser(chooser, copy, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(async () => ((await gridTexts()).length === 3 ? true : null), {
      timeout: 20000,
      message: "the document to reach the grid",
    });
    expect((await gridTexts())[0]).toBe(FIRST);
    expect(await present(".statusbar__dirty")).toBe(false);

    await runModuleItem(toplevel, MODULE_ITEM);
    await waitFor(async () => ((await gridTexts())[0] === WROTE ? true : null), {
      timeout: 20000,
      message: "the first cue to carry what the module wrote",
    });
    expect(await present(".statusbar__dirty")).toBe(true);
    // The module cannot write the file and there is no save on the boundary (4.5).
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);

    // One step, not one per cue the module touched: it went through the history like any edit.
    await clickElement(toplevel, ".toolbar__edit-undo");
    await waitFor(async () => ((await gridTexts())[0] === FIRST ? true : null), {
      timeout: 20000,
      message: "the undo to put the line back",
    });
    expect(await present(".statusbar__dirty")).toBe(false);
    expect(readFileSync(copy).equals(openedBytes)).toBe(true);
  });

  it("refuses a proposal made against a revision the session has moved past", async () => {
    // Criterion 6. The document is the one the check above left, and its revision has moved twice.
    const before = await gridTexts();

    await runModuleItem(toplevel, STALE_ITEM);
    // The refusal is the host's and the core has no sentence for it, so the log is where it is
    // said: item 4 is the fixture's own id for this one and 6 is SUBLORE_ERR_STALE_REVISION.
    await waitFor(
      () =>
        /modules: sublore_module_fixture\.so refused item 4, reporting 6/.test(appLog(dataHome())),
      { timeout: 20000, message: "the app to say it refused the stale proposal" },
    );
    expect(await gridTexts()).toEqual(before);
    expect(await present(".statusbar__dirty")).toBe(false);
  });

  it("says the same thing in About, where it stays", async () => {
    await clickElement(toplevel, ".menubar__title--help");
    await waitFor(() => present(".menubar__menu"), {
      timeout: 15000,
      message: "the Help menu to open",
    });
    await clickElement(toplevel, "#menuitem-help-about");
    await waitFor(() => present(".about"), {
      timeout: 15000,
      message: "the About panel to open",
    });

    const drawn = await textOf(".about__module--refused");
    expect(drawn).toContain(FIXTURE_FILE);
    expect(drawn).toContain(String(THEIRS));
    // Nothing loaded, so the only module line About carries is the refusal.
    expect(await present(".about__modules-heading")).toBe(true);

    pressKey("Escape");
    await waitFor(async () => ((await present(".about")) === false ? true : null), {
      timeout: 15000,
      message: "the About panel to close",
    });
  });

  it("keeps what the module wrote into its own table across a close and a reopen", async () => {
    // The item is a project's command, so with nothing open it is drawn and greyed rather than
    // absent, which is the ruling of 2026-09-03 seen from the module's side.
    await closeAnyOpenProject(toplevel);
    expect(await storeItemState(toplevel)).toEqual({ drawn: true, enabled: false });

    const folder = path.join(dataHome(), "module-store");
    rmSync(folder, { recursive: true, force: true });
    mkdirSync(folder, { recursive: true });
    await makeProjectIn(toplevel, folder);
    expect(await storeItemState(toplevel)).toEqual({ drawn: true, enabled: true });

    // Twice, so the second count says the first row was still there when the second was written.
    expect(await storeAndRead(toplevel)).toBe(1);
    expect(await storeAndRead(toplevel)).toBe(2);

    // Closed and opened again, which is what puts the rows through the file rather than through
    // one connection's memory.
    await openProjectMenu(toplevel);
    await chooseRailItem(toplevel, "close-project");
    await confirmRailDialog(toplevel);
    await waitFor(() => present(".rail__empty"), {
      timeout: 20000,
      message: "the rail to come back empty after the project was closed",
    });
    await openProjectMenu(toplevel);
    await chooseRailItem(toplevel, "open-project");
    const chooser = await waitForChooser("Choose a project folder");
    await answerChooser(chooser, folder, "project folder");
    focusWindow(toplevel.id);
    await waitFor(() => present(".rail__project"), {
      timeout: 20000,
      message: "the project to come back after it was reopened",
    });

    // Three, not one: the two rows written before the close are in the file.
    expect(await storeAndRead(toplevel)).toBe(3);

    // And the core still reads its own tables through the same file, which is what the guard is
    // for: a project a module had written is a project the app still opens.
    await openProjectMenu(toplevel);
    await chooseRailItem(toplevel, "add-episode");
    await browser.execute(() => {
      const field = document.querySelector(".raildialog__field");
      if (field !== null) {
        field.focus();
      }
    });
    typeText("One");
    await confirmRailDialog(toplevel);
    await waitFor(() => present(".rail__episode"), {
      timeout: 20000,
      message: "the episode to reach the rail of a project a module has written to",
    });
  });
});
