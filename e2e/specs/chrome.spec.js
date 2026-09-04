/* global describe, it, before, document, window */
/**
 * M2.0's T3: the menu bar and the toolbar that replaced the command bars M0 to M4 bolted on.
 *
 * What is asserted here is the part no other spec touches: which titles exist, that a command
 * reaches both routes, and the keyboard model in shell-layout.md — Alt opens, the arrows walk,
 * Enter activates, Escape closes and hands the keyboard back. The keys are driven as keys; nothing
 * here asks whether a handler is installed.
 *
 * Quitting from the menu is not here. Two of its answers end the process and a WebDriver session
 * cannot report that, so `e2e/scripts/quit-gate-check.js` owns the Quit item and drives it the same
 * way this file drives the rest.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, cancelChooser, chooserClosed, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/**
 * The whole bar, in the order it draws it, with the greying each title carries while nothing is
 * open. A title is drawn whether or not anything behind it can be used, so Audio is here too,
 * greyed until a media with tracks is open (owner ruling 2026-09-03, which reverses decision 24 A2).
 */
const TITLES = [
  { id: "file", label: "File", disabled: false },
  { id: "edit", label: "Edit", disabled: false },
  { id: "subtitle", label: "Subtitles", disabled: false },
  { id: "timing", label: "Timing", disabled: false },
  { id: "view", label: "View", disabled: false },
  { id: "audio", label: "Audio", disabled: true },
  { id: "help", label: "Help", disabled: false },
];

/** The titles with something behind them, which are the only ones a click opens. */
const OPENING = TITLES.filter((title) => !title.disabled);

/** The File commands nothing open leaves usable. Each is drawn greyed rather than left out. */
const GREYED_IN_FILE = ["file-save", "file-save-copy", "file-discard"];

/** Every command the bars T3 removed used to offer. Each has to reach both routes. */
const FROM_THE_BARS = [
  "file-open-subtitle",
  "video-open",
  "file-save",
  "file-save-copy",
  "edit-undo",
  "edit-redo",
];

const NO_FILE_STATUS = "No subtitle file open.";

/** What ctrl+s writes into the first cue of the copy the menu opens. */
const EDITED_TEXT = "Saved with the keyboard";

/** Writes go to the harness temp dir: the committed fixture is copied, never opened for editing. */
function workingCopy() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "chrome");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "basic-lf.srt");
  copyFileSync(fixture("srt", "clean", "basic-lf.srt"), copy);
  return copy;
}

function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
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

async function clickElement(toplevel, selector) {
  const centre = await centreOf(selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

/** A drawn control's greying, or null when the control is not drawn at all. */
function disabledOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.disabled ?? null, selector);
}

/** The bar's titles in the order it draws them, each with the id its class carries. */
function titlesOnTheBar() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".menubar__title")).map((title) => ({
      id:
        Array.from(title.classList)
          .find((name) => name.startsWith("menubar__title--"))
          ?.replace("menubar__title--", "") ?? null,
      label: title.textContent,
      disabled: title.disabled,
    })),
  );
}

/** The title of the open dropdown, or null when none is open. */
function openMenu() {
  return browser.execute(
    () => document.querySelector(".menubar__menu")?.getAttribute("aria-label") ?? null,
  );
}

/** The command the menu cursor is on, by id, or null when no item carries it. */
function cursorCommand() {
  return browser.execute(
    () => document.querySelector(".menubar__item--cursor")?.id.replace("menuitem-", "") ?? null,
  );
}

/** The class the element holding the keyboard carries, which is how focus is named here. */
function focusedClass() {
  return browser.execute(() => document.activeElement?.className ?? null);
}

async function waitForCursor(wanted) {
  return waitFor(async () => ((await cursorCommand()) === wanted ? true : null), {
    timeout: 15000,
    message: `the menu cursor to sit on ${wanted}`,
  });
}

async function waitForOpenMenu(wanted) {
  return waitFor(async () => ((await openMenu()) === wanted ? true : null), {
    timeout: 15000,
    message: `the ${wanted} dropdown to be the open one`,
  });
}

async function waitForNoMenu() {
  return waitFor(async () => ((await openMenu()) === null ? true : null), {
    timeout: 15000,
    message: "the dropdown to close",
  });
}

/** The command ids the open dropdown holds, in the order it draws them. */
function itemsOfOpenMenu() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".menubar__item")).map((item) =>
      item.id.replace("menuitem-", ""),
    ),
  );
}

describe("the menu bar and the toolbar", () => {
  let toplevel = null;
  let opened = null;

  before(async () => {
    opened = workingCopy();
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

  it("draws every title with nothing open, greying the one with nothing behind it", async () => {
    // Nothing at all is open, so Audio has no track to list and File has nothing to save.
    expect(await textOf(".statusbar__document")).toBe(NO_FILE_STATUS);
    expect(await textOf(".stage__empty")).not.toBe(null);

    // The bar is whole in the emptiest state there is: Audio is drawn and greyed, not dropped.
    expect(await titlesOnTheBar()).toEqual(TITLES);

    // The same rule one level down, on the commands the owner went looking for: drawn, greyed.
    await clickElement(toplevel, ".menubar__title--file");
    await waitForOpenMenu("File");
    const drawn = await itemsOfOpenMenu();
    for (const id of GREYED_IN_FILE) {
      expect({
        id,
        drawn: drawn.includes(id),
        disabled: await disabledOf(`#menuitem-${id}`),
      }).toEqual({ id, drawn: true, disabled: true });
    }
    pressKey("Escape");
    await waitForNoMenu();
  });

  it("offers every command the removed bars offered, from the menu and from the toolbar", async () => {
    const inMenus = [];
    for (const title of OPENING) {
      await clickElement(toplevel, `.menubar__title--${title.id}`);
      await waitForOpenMenu(title.label);
      inMenus.push(...(await itemsOfOpenMenu()));
      pressKey("Escape");
      await waitForNoMenu();
    }
    const inToolbar = await browser.execute(() =>
      Array.from(document.querySelectorAll(".toolbar__button")).map((button) =>
        Array.from(button.classList)
          .filter((name) => name !== "toolbar__button")[0]
          .replace("toolbar__", ""),
      ),
    );

    // Nothing on the toolbar is missing from the menus, and every command the bars carried is on
    // both routes. Quit and About are menu-only, which is what a toolbar is for.
    expect(inToolbar.filter((id) => !inMenus.includes(id))).toEqual([]);
    for (const id of FROM_THE_BARS) {
      expect({ id, menu: inMenus.includes(id), toolbar: inToolbar.includes(id) }).toEqual({
        id,
        menu: true,
        toolbar: true,
      });
    }
  });

  it("opens the first dropdown on Alt, with the cursor on its first enabled item", async () => {
    pressKey("alt");
    await waitForOpenMenu("File");
    await waitForCursor("file-open-subtitle");

    expect(await focusedClass()).toContain("menubar__menu");

    pressKey("Escape");
    await waitForNoMenu();
  });

  it("walks the items with the arrows and steps over the disabled ones", async () => {
    // Nothing is open, so Save, Save a copy and Discard are the disabled run in the middle of File.
    pressKey("alt");
    await waitForCursor("file-open-subtitle");
    expect(
      await browser.execute(() => document.querySelector("#menuitem-file-save")?.disabled ?? null),
    ).toBe(true);

    pressKey("Down");
    await waitForCursor("video-open");
    pressKey("Down");
    await waitForCursor("app-quit");
    pressKey("Up");
    await waitForCursor("video-open");

    pressKey("Escape");
    await waitForNoMenu();
  });

  it("moves between the titles with left and right while a dropdown is open", async () => {
    pressKey("alt");
    await waitForOpenMenu("File");

    pressKey("Right");
    await waitForOpenMenu("Edit");
    pressKey("Right");
    await waitForOpenMenu("Subtitles");
    pressKey("Right");
    await waitForOpenMenu("Timing");
    pressKey("Right");
    await waitForOpenMenu("View");
    pressKey("Right");
    await waitForOpenMenu("Help");
    pressKey("Left");
    await waitForOpenMenu("View");

    pressKey("Escape");
    await waitForNoMenu();
  });

  it("closes on Escape and hands the keyboard back to where it was", async () => {
    await browser.execute(() => document.querySelector(".cuelist")?.focus());
    expect(await focusedClass()).toContain("cuelist");

    pressKey("alt");
    await waitForOpenMenu("File");
    expect(await focusedClass()).toContain("menubar__menu");

    pressKey("Escape");
    await waitForNoMenu();

    expect(await focusedClass()).toContain("cuelist");
  });

  it("activates the item under the cursor on Enter", async () => {
    pressKey("alt");
    await waitForOpenMenu("File");
    // File, Edit, Subtitles, Timing, View, Help: the walk the test above asserts, taken here to
    // reach About. Audio is skipped because with nothing open it has no track to list.
    pressKey("Right");
    pressKey("Right");
    pressKey("Right");
    pressKey("Right");
    pressKey("Right");
    await waitForOpenMenu("Help");
    await waitForCursor("help-about");

    pressKey("Return");
    await waitFor(() => present(".about"), {
      timeout: 15000,
      message: "the About panel to open",
    });
    expect(await openMenu()).toBe(null);
    expect(await textOf(".about__title")).toBe("About Sublore");

    pressKey("Escape");
    await waitFor(async () => ((await present(".about")) === false ? true : null), {
      timeout: 15000,
      message: "the About panel to close",
    });
  });

  it("raises the subtitle chooser on ctrl+o and leaves the document alone when it is dismissed", async () => {
    expect(await textOf(".statusbar__document")).toBe(NO_FILE_STATUS);

    pressKey("ctrl+o");
    const chooser = await waitForChooser("Choose a subtitle");
    await cancelChooser(chooser, "subtitle");
    focusWindow(toplevel.id);

    expect(await textOf(".statusbar__document")).toBe(NO_FILE_STATUS);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("raises the video chooser on ctrl+shift+o and leaves the stage alone when it is dismissed", async () => {
    const empty = await textOf(".stage__empty");
    expect(empty).not.toBe(null);

    pressKey("ctrl+shift+o");
    const chooser = await waitForChooser("Choose a video");
    await cancelChooser(chooser, "video");
    focusWindow(toplevel.id);

    expect(await textOf(".stage__empty")).toBe(empty);
    expect(await textOf(".statusbar__video-error")).toBe(null);
  });

  it("opens a subtitle through the File menu", async () => {
    pressKey("alt");
    await waitForCursor("file-open-subtitle");

    pressKey("Return");
    const chooser = await waitForChooser("Choose a subtitle");
    await answerChooser(chooser, opened, "subtitle");
    focusWindow(toplevel.id);

    await waitFor(async () => (await textOf(".statusbar__document"))?.startsWith("SRT") === true, {
      timeout: 20000,
      message: "the status bar to report the subtitle the menu opened",
    });
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("draws the same titles, greying and all, once a document is open", async () => {
    // The state is opened here rather than inherited, so a failure means the bar moved and not that
    // an earlier check left something behind. A dropdown left down would sit over the toolbar.
    pressKey("Escape");
    await waitForNoMenu();
    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const chooser = await waitForChooser("Choose a subtitle");
    await answerChooser(chooser, opened, "subtitle");
    focusWindow(toplevel.id);
    await waitFor(async () => (await textOf(".statusbar__document"))?.startsWith("SRT") === true, {
      timeout: 20000,
      message: "the status bar to report the subtitle the toolbar opened",
    });

    // Not one title more and not one fewer than the empty app drew, and Audio is still greyed: a
    // subtitle brings no audio tracks with it. What an open moves is the greying inside the menus.
    expect(await titlesOnTheBar()).toEqual(TITLES);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("raises the save-a-copy chooser on ctrl+shift+s and writes nothing when it is dismissed", async () => {
    const status = await textOf(".statusbar__document");

    pressKey("ctrl+shift+s");
    const chooser = await waitForChooser("Save a copy of the subtitle");
    await cancelChooser(chooser, "subtitle-save");
    expect(await chooserClosed(chooser)).toBe(true);
    focusWindow(toplevel.id);

    expect(await textOf(".statusbar__document")).toBe(status);
    expect(await textOf(".statusbar__message")).toBe(null);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("saves the open file on ctrl+s, which the menu draws beside Save", async () => {
    const before = readFileSync(opened, "utf8");
    await clickElement(toplevel, ".cuelist__row .cuelist__text");
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open on the first row",
    });
    pressKey("ctrl+a");
    typeText(EDITED_TEXT);
    pressKey("Return");
    await waitFor(() => present(".statusbar__dirty"), {
      timeout: 20000,
      message: "the edit to reach the document",
    });

    pressKey("ctrl+s");
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the dirty marker to clear after ctrl+s",
    });

    const after = readFileSync(opened, "utf8");
    expect(after).toContain(EDITED_TEXT);
    // The edit and nothing else: one block differs, and it differs only in its text.
    const beforeBlocks = before.split("\n\n");
    const afterBlocks = after.split("\n\n");
    expect(afterBlocks.length).toBe(beforeBlocks.length);
    expect(afterBlocks.filter((block, index) => block !== beforeBlocks[index]).length).toBe(1);
    expect(await textOf(".statusbar__error")).toBe(null);
  });
});
