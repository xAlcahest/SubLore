/* global describe, it, before, document, window */
import { Buffer } from "node:buffer";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
} from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow, rightClickAt, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { closeAnyOpenProject } from "../lib/rail.js";
import { findToplevel } from "../lib/x11.js";

/** Copies of src/i18n/en.ts. The harness cannot import TypeScript, so the strings are pinned here. */
const NO_PROJECT = "No project open.";
const NO_PROJECT_HERE = "There is no Sublore project in that folder.";
const ALREADY_ATTACHED = "That file is already attached to this episode.";
const MISSING = "missing";
/** From `en.project.roles`: what the attachment's tooltip calls it. */
const SOURCE_ROLE = "Source";
const EPISODE_TITLE = "Episode 1";
// ASCII on purpose: `xdotool type` remaps a spare keycode for a character the keymap has no
// key for, and the webview reports it as unidentified, so the field never fills. Non-ASCII
// titles are covered where they belong, in `records.rs`'s `renames_an_episode`.
const RENAMED_TITLE = "Episode 1 pilot";
/** What the episode row reads, from `en.project.episode`: "{ordinal}. {title}". */
const EPISODE_ROW = `1. ${EPISODE_TITLE}`;
const RENAMED_ROW = `1. ${RENAMED_TITLE}`;

/** The one file Sublore writes into a project folder, from crates/sublore-project/src/layout.rs. */
const DATABASE_NAME = "project.sublore";

/** Subtitle fixtures are committed: a missing one is a broken checkout, never a skip. */
function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
}

/** Everything this spec writes lives under the harness temp dir, never in the repo. */
function scratch(name) {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "project", name);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
}

/** Centre of an element in physical pixels, which is what X11 pointer coordinates are. */
function centreOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    // The rail is only as tall as the top block, so a node further down the tree has to be brought
    // into view before a pointer can reach it. See T2's full-width grid.
    element.scrollIntoView({ block: "nearest", inline: "nearest" });
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, selector);
}

async function pointAt(toplevel, selector) {
  const centre = await centreOf(selector);
  if (centre === null) {
    throw new Error(`${selector} is missing from the DOM`);
  }
  // No window manager under Xvfb, so the toplevel origin is also the viewport origin.
  return { x: toplevel.absX + centre.x, y: toplevel.absY + centre.y };
}

async function clickElement(toplevel, selector) {
  const point = await pointAt(toplevel, selector);
  clickAt(point.x, point.y);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function textsOf(selector) {
  return browser.execute(
    (css) => Array.from(document.querySelectorAll(css)).map((node) => node.textContent),
    selector,
  );
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function attributeOf(selector, name) {
  return browser.execute(
    (css, attribute) => document.querySelector(css)?.getAttribute(attribute) ?? null,
    selector,
    name,
  );
}

/** The project node's own menu: project commands, and the one that adds an episode. */
async function openProjectMenu(toplevel) {
  const open = await present(".rail__project");
  await clickElement(toplevel, open ? ".rail__project" : ".rail__empty");
  await waitFor(() => present(".railmenu"), {
    timeout: 15000,
    message: "the rail's project menu to open",
  });
}

/** An episode's or a file's own menu, which is where its commands live (decision 24, A3). */
async function openMenuOn(toplevel, selector) {
  const point = await pointAt(toplevel, selector);
  rightClickAt(point.x, point.y);
  await waitFor(() => present(".railmenu"), {
    timeout: 15000,
    message: `the context menu on ${selector} to open`,
  });
}

async function chooseMenuItem(toplevel, key) {
  await clickElement(toplevel, `.railmenu__item--${key}`);
  await waitFor(async () => (await present(".railmenu")) === false, {
    timeout: 15000,
    message: `the menu to close after ${key}`,
  });
}

/** Type a name into the one question the rail asks, which opens with its field already focused. */
async function typeIntoDialog(text) {
  await waitFor(
    () => browser.execute(() => document.activeElement?.matches(".raildialog__field") === true),
    { timeout: 15000, message: "the dialog's field to take keyboard focus" },
  );
  execFileSync("xdotool", ["key", "--clearmodifiers", "ctrl+a"], {
    encoding: "utf8",
    timeout: 15000,
  });
  typeText(text);
  // Also proves the select-all landed: leftover text would make this value wrong, not just longer.
  await waitFor(
    () =>
      browser.execute((want) => document.querySelector(".raildialog__field")?.value === want, text),
    { timeout: 15000, message: `the dialog's field to hold exactly ${text}` },
  );
}

async function confirmDialog(toplevel) {
  await clickElement(toplevel, ".raildialog__confirm");
  await waitFor(async () => (await present(".raildialog")) === false, {
    timeout: 15000,
    message: "the dialog to close after it was confirmed",
  });
}

/** Choose the project folder through the chooser the menu item raises; nothing is typed. */
async function chooseFolder(toplevel, key, folder) {
  await openProjectMenu(toplevel);
  await chooseMenuItem(toplevel, key);
  const chooser = await waitForChooser("Choose a project folder");
  await answerChooser(chooser, folder, "project folder");
  focusWindow(toplevel.id);
}

/** Choose a file for an episode or for an attachment, through the same chooser the app raises. */
async function chooseFile(toplevel, menuSelector, key, file) {
  await openMenuOn(toplevel, menuSelector);
  await chooseMenuItem(toplevel, key);
  const chooser = await waitForChooser("Choose a video or subtitle file");
  await answerChooser(chooser, file, "episode file");
  focusWindow(toplevel.id);
}

/**
 * Which project is open and where. The rail is 104px wide, so the node carries the folder's own
 * name and the whole path is its tooltip.
 */
async function waitForFolder(expected) {
  return waitFor(
    async () => {
      const folder = await attributeOf(".rail__project", "title");
      return folder !== null && folder.includes(expected) ? folder : null;
    },
    { timeout: 20000, message: `the rail to say the open project is at ${expected}` },
  );
}

async function waitForError() {
  return waitFor(
    async () => {
      const text = await textOf(".statusbar__project-error");
      return text !== null && text.trim() !== "" ? text : null;
    },
    { timeout: 20000, message: "the project error line to appear" },
  );
}

async function waitForEpisodeRow(row) {
  return waitFor(
    async () => (await textsOf(".rail__episode")).some((text) => text?.includes(row)),
    {
      timeout: 20000,
      message: `the rail to hold the episode row ${JSON.stringify(row)}`,
    },
  );
}

/**
 * What each attached file is and where, which the rows carry as their tooltip because a rail row is
 * only as wide as the rail itself. From `en.project.file`: "{role} · {path}".
 */
function attachedFiles() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".rail__file")).map((node) => node.getAttribute("title")),
  );
}

function sourceLine(file) {
  return `${SOURCE_ROLE} · ${file}`;
}

/** The app window, after a launch or a relaunch. Also proves exactly one instance is running. */
async function attachToApp() {
  const toplevel = await waitFor(findToplevel, {
    timeout: 40000,
    message: `exactly one ${windowWidth}x${windowHeight} "Sublore" toplevel to be on screen`,
  });
  focusWindow(toplevel.id);
  await waitFor(() => browser.execute(() => document.querySelector(".rail") !== null), {
    timeout: 40000,
    message: "the project rail to render",
  });
  return toplevel;
}

describe("projects", () => {
  let toplevel = null;
  let projectFolder = null;
  let userFolder = null;
  let awayFolder = null;
  let emptyFolder = null;
  let userSubtitle = null;
  let sourceFixture = null;

  before(async () => {
    sourceFixture = fixture("srt", "clean", "basic-lf.srt");
    projectFolder = scratch("series");
    userFolder = scratch("user-media");
    awayFolder = scratch("user-media-moved");
    emptyFolder = scratch("not-a-project");
    // A copy outside the project folder, so the deletion test asserts on a real user file rather
    // than on a repo fixture the app must never be able to reach.
    userSubtitle = path.join(userFolder, "ep01.srt");
    copyFileSync(sourceFixture, userSubtitle);

    toplevel = await attachToApp();
    await closeAnyOpenProject(toplevel);
  });

  it("creates a project in an empty folder", async () => {
    expect(await textOf(".rail__empty")).toBe(NO_PROJECT);

    await chooseFolder(toplevel, "create-project", projectFolder);

    expect(await waitForFolder(projectFolder)).toContain(projectFolder);
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect(existsSync(path.join(projectFolder, DATABASE_NAME))).toBe(true);
  });

  it("adds an episode and attaches a subtitle file to it", async () => {
    const before_ = statSync(userSubtitle);

    await openProjectMenu(toplevel);
    await chooseMenuItem(toplevel, "add-episode");
    await typeIntoDialog(EPISODE_TITLE);
    await confirmDialog(toplevel);
    await waitForEpisodeRow(EPISODE_ROW);

    await chooseFile(toplevel, ".rail__episode", "attach-source", userSubtitle);
    await waitFor(async () => (await attachedFiles()).includes(sourceLine(userSubtitle)), {
      timeout: 20000,
      message: `the rail to hold an attachment for ${userSubtitle}`,
    });

    expect(await textOf(".statusbar__project-error")).toBe(null);
    // Attaching records a path. CONTRIBUTING.md §3.1: the user's file is not read for writing, not
    // copied, not moved, not touched.
    const after = statSync(userSubtitle);
    expect(after.size).toBe(before_.size);
    expect(after.mtimeMs).toBe(before_.mtimeMs);
    expect(Buffer.compare(readFileSync(userSubtitle), readFileSync(sourceFixture))).toBe(0);
  });

  it("reopens the project it had open, on the episode it was on, and opens nothing else", async () => {
    // A real relaunch: the WebDriver session is deleted, which ends the process, and the next one
    // starts the binary again. Nothing of the old run survives but what was written to disk.
    //
    // Proof that this is a new page and not the one that was already holding the project: a mark
    // left on the window object is gone afterwards. The old check proved the same thing by
    // asserting that a fresh Sublore opens nothing, which decision 24 D5 has since reversed — a
    // launch now reopens the project that was open, so the proof moved rather than went.
    await browser.execute(() => {
      window.name = "before-the-relaunch";
    });
    await browser.reloadSession();
    toplevel = await attachToApp();
    expect(await browser.execute(() => window.name)).not.toBe("before-the-relaunch");

    await waitForFolder(projectFolder);
    const episodes = await textsOf(".rail__episode");
    expect(episodes).toHaveLength(1);
    expect(episodes[0]).toContain(EPISODE_ROW);
    expect(await attachedFiles()).toEqual([sourceLine(userSubtitle)]);
    expect(await attributeOf(".rail__episode", "aria-current")).toBe("true");
    expect(await textOf(".statusbar__project-error")).toBe(null);

    // D5 is explicit that the reopen opens the project and nothing in it.
    expect(await textOf(".statusbar__document")).toBe("No subtitle file open.");
    expect(await present(".stage__empty")).toBe(true);
  });

  it("reports a folder that holds no project and stays usable", async () => {
    await chooseFolder(toplevel, "open-project", emptyFolder);

    expect(await waitForError()).toContain(NO_PROJECT_HERE);
    expect(await present(".rail__project")).toBe(false);
    expect(existsSync(path.join(emptyFolder, DATABASE_NAME))).toBe(false);

    // Still usable: the real project opens straight afterwards, with the error line gone.
    await chooseFolder(toplevel, "open-project", projectFolder);
    await waitForFolder(projectFolder);
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect((await textsOf(".rail__episode"))[0]).toContain(EPISODE_ROW);

    // An attach that fails changed nothing, so the project stays on screen beside the message.
    // The failure staged here used to be a file deleted between the choosing and the attaching;
    // T1 and T7 closed that gap by making one gesture of both, so the attach a user can still make
    // fail is the one that names a file the episode already holds. The vanished-file refusal keeps
    // its coverage in `attach_refuses_a_path_it_cannot_record_honestly`, at the layer it happens in.
    await chooseFile(toplevel, ".rail__episode", "attach-target", userSubtitle);
    expect(await waitForError()).toContain(ALREADY_ATTACHED);
    expect(await attributeOf(".rail__project", "title")).toContain(projectFolder);
    expect((await textsOf(".rail__episode"))[0]).toContain(EPISODE_ROW);
    expect(await attachedFiles()).toEqual([sourceLine(userSubtitle)]);
  });

  it("renames an episode and touches no file on disk", async () => {
    const before_ = statSync(userSubtitle);

    await openMenuOn(toplevel, ".rail__episode");
    await chooseMenuItem(toplevel, "rename-episode");
    await typeIntoDialog(RENAMED_TITLE);
    await confirmDialog(toplevel);

    await waitForEpisodeRow(RENAMED_ROW);
    expect(await textsOf(".rail__episode")).toHaveLength(1);
    expect(await attachedFiles()).toEqual([sourceLine(userSubtitle)]);
    expect(await textOf(".statusbar__project-error")).toBe(null);
    const after = statSync(userSubtitle);
    expect(after.mtimeMs).toBe(before_.mtimeMs);
  });

  it("marks an attachment whose file has gone and points it at the file again", async () => {
    // The user moved their own file, which Sublore never does and never notices until it looks.
    const moved = path.join(awayFolder, "ep01.srt");
    renameSync(userSubtitle, moved);
    userSubtitle = moved;

    // The missing mark is read when the view is built, so the view is built again.
    await chooseFolder(toplevel, "open-project", projectFolder);
    await waitFor(() => present(".rail__file--missing"), {
      timeout: 20000,
      message: "the attachment to be marked missing once its file is gone",
    });
    expect(await textOf(".rail__missing")).toBe(MISSING);
    // Never dropped: the record still holds the path it was given.
    expect(await attachedFiles()).toEqual([sourceLine(path.join(userFolder, "ep01.srt"))]);

    await chooseFile(toplevel, ".rail__file", "locate-file", moved);
    await waitFor(async () => (await present(".rail__file--missing")) === false, {
      timeout: 20000,
      message: "the mark to clear once the record points at the file again",
    });
    expect(await attachedFiles()).toEqual([sourceLine(moved)]);
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect(Buffer.compare(readFileSync(moved), readFileSync(sourceFixture))).toBe(0);
  });

  it("opens an attached subtitle from the rail", async () => {
    expect(await textOf(".statusbar__document")).toBe("No subtitle file open.");

    await clickElement(toplevel, ".rail__file");

    await waitFor(async () => (await textOf(".statusbar__document"))?.startsWith("SRT") === true, {
      timeout: 20000,
      message: "the status line to report the subtitle the rail opened",
    });
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await textsOf(".cuelist__row")).not.toHaveLength(0);
  });

  it("detaches a file and leaves it on disk", async () => {
    const before_ = statSync(userSubtitle);

    await openMenuOn(toplevel, ".rail__file");
    await chooseMenuItem(toplevel, "detach-file");
    await confirmDialog(toplevel);

    await waitFor(async () => (await present(".rail__file")) === false, {
      timeout: 20000,
      message: "the attachment row to go",
    });
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect((await textsOf(".rail__episode"))[0]).toContain(RENAMED_ROW);
    expect(existsSync(userSubtitle)).toBe(true);
    expect(statSync(userSubtitle).mtimeMs).toBe(before_.mtimeMs);
    expect(Buffer.compare(readFileSync(userSubtitle), readFileSync(sourceFixture))).toBe(0);
  });

  it("deletes an episode and leaves the project open", async () => {
    await openMenuOn(toplevel, ".rail__episode");
    await chooseMenuItem(toplevel, "delete-episode");
    await confirmDialog(toplevel);

    await waitFor(async () => (await present(".rail__episode")) === false, {
      timeout: 20000,
      message: "the episode row to go",
    });
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect(await attributeOf(".rail__project", "title")).toContain(projectFolder);
    expect(existsSync(userSubtitle)).toBe(true);
  });

  it("closes the project, and the next launch opens nothing", async () => {
    await openProjectMenu(toplevel);
    await chooseMenuItem(toplevel, "close-project");
    await confirmDialog(toplevel);

    await waitFor(() => present(".rail__empty"), {
      timeout: 20000,
      message: "the rail to empty once the project is closed",
    });
    expect(existsSync(path.join(projectFolder, DATABASE_NAME))).toBe(true);

    await browser.reloadSession();
    toplevel = await attachToApp();
    expect(await textOf(".rail__empty")).toBe(NO_PROJECT);
    expect(await present(".rail__project")).toBe(false);
  });

  it("deletes the project without touching the files it points at", async () => {
    await chooseFolder(toplevel, "open-project", projectFolder);
    await waitForFolder(projectFolder);

    await openProjectMenu(toplevel);
    await chooseMenuItem(toplevel, "delete-project");
    // The folder is named in the question, so nothing is deleted straight off a click (D2).
    expect(await textOf(".raildialog__message")).toContain(projectFolder);
    await confirmDialog(toplevel);

    await waitFor(() => !existsSync(path.join(projectFolder, DATABASE_NAME)), {
      timeout: 20000,
      message: `${DATABASE_NAME} to be gone from ${projectFolder}`,
    });
    expect(await textOf(".statusbar__project-message")).toContain(projectFolder);
    expect(await textOf(".statusbar__project-error")).toBe(null);
    expect(await present(".rail__project")).toBe(false);

    // The whole point of M4.3: the user's own subtitle file is exactly where it was, byte for byte.
    expect(existsSync(userSubtitle)).toBe(true);
    expect(Buffer.compare(readFileSync(userSubtitle), readFileSync(sourceFixture))).toBe(0);
    // And the folder the user chose is still theirs.
    expect(existsSync(projectFolder)).toBe(true);
  });
});
