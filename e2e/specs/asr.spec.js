/* global describe, it, before, document, window */
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import {
  appDataDir,
  damageModel,
  forgetStubRun,
  processLine,
  scratchRuns,
  setStubMode,
  stubArgv,
  stubPid,
} from "../lib/asr.js";
import { answerChooser, cancelChooser, findChooser, waitForChooser } from "../lib/chooser.js";
import { answerDialog, waitForUnsavedDialog, waitForUnsavedDialogGone } from "../lib/gtk-dialog.js";
import { clickAt, focusWindow, pressKey, typeText } from "../lib/input.js";
import {
  closeWindowTool,
  repoRoot,
  requireCloseWindowTool,
  requireVideoFixture,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** What the status line says before anything has been transcribed. */
const IDLE_STATUS = "No transcription yet.";
/** A word tiny.en got right in the capture the stub sidecar replays. */
const HEARD_WORD = "terminology";
/** The fixture is 60 s of tone; every generated cue has to sit inside it. */
const FIXTURE_MS = 60000;
/** What is typed over a cue of the transcription, and over one of the file it was saved to. */
const CORRECTION = "Corrected by hand in the grid";
const SECOND_EDIT = "Edited while a transcription was running";
/** Typed after a first save, to prove the save that follows writes the file it adopted. */
const AFTER_FIRST_SAVE = "Edited after the first save";
/** Typed into the document a transcription is about to replace, so its save can be found on disk. */
const IN_THE_WAY = "Work in the way of a transcription";
/** The save chooser a document with no file raises. Frozen contract with src-tauri/src/strings.rs. */
const FIRST_SAVE_TITLE = "Save the subtitle";

/** Writes go to the harness temp dir, never into the repo and never beside a fixture. */
function saveDirectory(name) {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, name);
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

function propertyOf(selector, property) {
  return browser.execute(
    (css, name) => {
      const element = document.querySelector(css);
      return element === null ? null : (element[name] ?? null);
    },
    selector,
    property,
  );
}

/** Every cue the app is showing, as the user sees them. */
function shownCues() {
  return browser.execute(() =>
    [...document.querySelectorAll(".asrbar__cue")].map((cue) => ({
      start: Number(cue.dataset.start),
      end: Number(cue.dataset.end),
      text: cue.textContent ?? "",
    })),
  );
}

async function waitForStatus(fragment, timeout = 30000) {
  return waitFor(
    async () => {
      const status = await textOf(".asrbar__status");
      if (status !== null && status.includes(fragment)) {
        return status;
      }
      // An error banner means this wait will never end; carry its text into the timeout message
      // so a failure says what went wrong instead of only what did not happen.
      const failure = await textOf(".asrbar__error");
      if (failure !== null) {
        throw new Error(`the app reported: ${failure}`);
      }
      return null;
    },
    { timeout, message: `the transcription status line to contain ${JSON.stringify(fragment)}` },
  );
}

/**
 * A cue's words, however the list that holds them lays them out: the bar joins a wrapped cue's two
 * lines with a space and draws a timestamp in front, the grid keeps the line break.
 */
function words(text) {
  return (text ?? "")
    .replace(/^\d+:\d\d/, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** The text of the grid row at a 1-based list position, or null when it is not rendered. */
function rowText(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    return row?.querySelector(".cuelist__text")?.textContent ?? null;
  }, String(position));
}

/** Centre of the text cell of the row at a 1-based list position, if it is rendered. */
function centreOfRow(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    const cell = row?.querySelector(".cuelist__text");
    if (!cell) {
      return null;
    }
    const rect = cell.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: (rect.x + rect.width / 2) * dpr, y: (rect.y + rect.height / 2) * dpr };
  }, String(position));
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/** The command the menu cursor is on, by id, or null when no item carries it. */
function cursorCommand() {
  return browser.execute(
    () => document.querySelector(".menubar__item--cursor")?.id.replace("menuitem-", "") ?? null,
  );
}

/** The title of the open dropdown, or null when none is open. */
function openDropdown() {
  return browser.execute(
    () => document.querySelector(".menubar__menu")?.getAttribute("aria-label") ?? null,
  );
}

/** Type over a cue in the grid and commit it, which is the editor the milestone promises. */
async function editRow(toplevel, position, text) {
  const centre = await centreOfRow(position);
  if (centre === null) {
    throw new Error(`row ${position} is not rendered`);
  }
  clickAt(toplevel.absX + centre.x, toplevel.absY + centre.y);
  await waitFor(() => present(".cuelist__editor"), {
    timeout: 15000,
    message: `the inline editor to open on row ${position}`,
  });
  pressKey("ctrl+a");
  typeText(text);
  pressKey("Return");
  await waitFor(async () => ((await rowText(position)) === text ? true : null), {
    timeout: 20000,
    message: `row ${position} to hold ${JSON.stringify(text)}`,
  });
  // The row drawing the new text and the document being unsaved are two different states, and the
  // checks below turn on the second: a run that finishes before it lands replaces a document
  // nothing had to ask about. Waited for rather than assumed. See BACKLOG.md N25.
  await waitFor(async () => ((await present(".statusbar__dirty")) ? true : null), {
    timeout: 20000,
    message: `the document to be unsaved after row ${position} was committed`,
  });
}

async function waitForSubtitleStatus(prefix) {
  return waitFor(
    async () => {
      const status = await textOf(".statusbar__document");
      return status !== null && status.startsWith(prefix) ? status : null;
    },
    { timeout: 20000, message: `the subtitle status line to start with ${JSON.stringify(prefix)}` },
  );
}

/**
 * Open the transcription panel the way T4 says it opens: from the menu, driven as keys.
 *
 * Keys and not clicks because a dropdown hangs over the video rectangle, and a click there lands on
 * the native surface instead of the webview — measured on Linux, and what decision 1's occlusion
 * (T8) is for. Alt opens File, Right moves to Edit, and Up wraps to its last enabled item rather
 * than counting Downs, because Undo and Redo are enabled in some of these tests and not in others.
 * The wait on the cursor is what makes the route explicit: it fails loudly if the item moves.
 */
async function openPanelFromMenu(toplevel) {
  focusWindow(toplevel.id);
  pressKey("alt");
  await waitFor(async () => ((await openDropdown()) === "File" ? true : null), {
    timeout: 15000,
    message: "the File dropdown to open on Alt",
  });
  pressKey("Right");
  await waitFor(async () => ((await openDropdown()) === "Edit" ? true : null), {
    timeout: 15000,
    message: "the Edit dropdown to be the open one",
  });
  pressKey("Up");
  await waitFor(async () => ((await cursorCommand()) === "asr-transcribe" ? true : null), {
    timeout: 15000,
    message: "the menu cursor to sit on Transcribe, the last item in Edit",
  });
  pressKey("Return");
  return waitFor(() => present(".asrbar__model"), {
    timeout: 15000,
    message: "the transcription panel the menu item opens",
  });
}

/** Start a run and wait until the sidecar has actually been spawned. */
async function startRun(toplevel) {
  forgetStubRun();
  await clickElement(toplevel, ".asrbar__start");
  return waitFor(() => stubPid(), {
    timeout: 30000,
    message: "the stand-in sidecar to be spawned and record its pid",
  });
}

/** The folder the media lives in, and what was in it before any run: the read-only guarantee is
 * about what a transcription adds, not about the list being frozen. See CONTRIBUTING.md §3.1. */
const videoDir = path.join(repoRoot, "fixtures", "video");
let mediaFolderBefore = [];

describe("transcription", () => {
  let toplevel = null;
  let fixture = null;
  let saveDir = null;
  /** Where the first-save tests write, kept apart so "nothing was written" is exact. */
  let firstSaveDir = null;
  /** The file a first save gave the document, which the save after it has to write to. */
  let adopted = null;
  /** The bytes the transcription was saved to, so a later step can prove they did not move. */
  let saved = null;

  before(async () => {
    mediaFolderBefore = readdirSync(videoDir).sort();
    fixture = requireVideoFixture();
    saveDir = saveDirectory("transcription-save");
    firstSaveDir = saveDirectory("first-save");
    requireCloseWindowTool();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    // The transcription controls are not here to be waited on any more (T4), so the gate is the
    // chrome that is: the panel is opened by the first test, from the menu.
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__video-open") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
  });

  // T4: nothing transcription-shaped is on screen until the menu is asked for it.
  it("draws no transcription control until the menu opens the panel", async () => {
    expect(await present(".asrpanel")).toBe(false);
    expect(await propertyOf(".asrbar__model", "tagName")).toBe(null);
    expect(await propertyOf(".asrbar__start", "tagName")).toBe(null);
    expect(await textOf(".asrbar__status")).toBe(null);

    await openPanelFromMenu(toplevel);

    expect(await present(".asrpanel")).toBe(true);
    expect(await propertyOf(".asrbar__model", "tagName")).toBe("SELECT");
  });

  it("offers the models it knows and a compute choice", async () => {
    const models = await browser.execute(() => {
      const select = document.querySelector(".asrbar__model");
      return {
        value: select.value,
        options: [...select.options].map((option) => ({ id: option.value, label: option.text })),
      };
    });

    // The whole catalog is offered, and the one that is on disk is the one already chosen.
    expect(models.options.length).toBeGreaterThanOrEqual(10);
    expect(models.options.map((option) => option.id)).toContain("tiny.en");
    expect(models.value).toBe("tiny.en");
    const ready = models.options.find((option) => option.id === "tiny.en");
    expect(ready.label.toLowerCase()).toContain("ready");

    // A model that is already on disk offers no download, so nothing here can reach the network.
    expect(await propertyOf(".asrbar__download", "tagName")).toBe(null);

    expect(await propertyOf(".asrbar__gpu", "checked")).toBe(true);
    // Nothing to transcribe until a video is open.
    expect(await propertyOf(".asrbar__start", "disabled")).toBe(true);
    expect(await textOf(".asrbar__status")).toBe(IDLE_STATUS);
  });

  it("transcribes the open video and shows the cues", async () => {
    await clickElement(toplevel, ".toolbar__video-open");
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, fixture, "video");
    // The chooser took the keyboard with it; the transcribe controls below are clicked, not typed,
    // but the app window has to be the focused one again before any of them can answer.
    focusWindow(toplevel.id);
    await waitFor(
      () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".asrbar__start")?.disabled === false,
        ),
      { timeout: 30000, message: "the fixture to load and enable Transcribe" },
    );

    // CONTRIBUTING.md §3.1: the user's media is read only. Snapshot what is beside it, and its own
    // metadata, and compare after the run.
    const before = { listing: readdirSync(videoDir).sort(), stat: statSync(fixture) };

    setStubMode("fast");
    await startRun(toplevel);
    const status = await waitForStatus(" cues");

    const cues = await shownCues();
    expect(cues.length).toBeGreaterThan(0);
    expect(status).toContain(`${cues.length} cues`);
    // The GPU box is still ticked, and the stand-in sidecar answers for both binaries.
    expect(status).toContain("GPU");
    expect(await textOf(".asrbar__error")).toBe(null);

    // Real whisper output came through the real parser and the real segmentation rule.
    expect(cues.map((cue) => cue.text).join(" ")).toContain(HEARD_WORD);
    let previousEnd = -1;
    for (const cue of cues) {
      expect(cue.start).toBeGreaterThanOrEqual(previousEnd);
      expect(cue.end).toBeGreaterThan(cue.start);
      expect(cue.end).toBeLessThanOrEqual(FIXTURE_MS);
      previousEnd = cue.end;
    }

    // The sidecar was handed the extracted audio out of the app's own scratch directory, never
    // the user's file, and the scratch directory is gone again afterwards.
    const argv = stubArgv();
    expect(argv).not.toContain(fixture);
    const input = argv[argv.indexOf("-f") + 1];
    expect(input.startsWith(appDataDir())).toBe(true);
    expect(input.endsWith("audio.wav")).toBe(true);
    expect(scratchRuns()).toEqual([]);

    const after = { listing: readdirSync(videoDir).sort(), stat: statSync(fixture) };
    expect(after.listing).toEqual(before.listing);
    expect(after.stat.size).toBe(before.stat.size);
    expect(after.stat.mtimeMs).toBe(before.stat.mtimeMs);
  });

  // BACKLOG.md M3.5: the run above finished, so its cues are the document. Nothing else opened it.
  it("leaves the cues it produced as the open document, unsaved and nowhere on disk", async () => {
    const cues = await shownCues();
    const status = await waitForSubtitleStatus(`SRT · ${cues.length} cues · LF`);

    expect(status).toContain(`${cues.length} cues`);
    expect(words(await rowText(1))).toBe(words(cues[0].text));
    // Unsaved from the first moment: these bytes exist nowhere but in the session.
    expect(await present(".statusbar__dirty")).toBe(true);
    // Both saves are offered: Save asks where a document with no file goes (decision 24, B2).
    expect(await propertyOf(".toolbar__file-save", "disabled")).toBe(false);
    expect(await propertyOf(".toolbar__file-save-copy", "disabled")).toBe(false);
    expect(await textOf(".statusbar__error")).toBe(null);

    // The transcription wrote nothing at all: not beside the media, and nowhere Sublore saves.
    // Compared against what was there when the run started, not against a list written here: a
    // fixture added to that folder is not a defect, and a frozen list would call it one.
    expect(readdirSync(saveDir)).toEqual([]);
    expect(readdirSync(videoDir).sort()).toEqual(mediaFolderBefore);
  });

  it("edits a cue of the result, saves it, and reopens the file with the edit in it", async () => {
    const cues = await shownCues();
    await editRow(toplevel, 1, CORRECTION);
    expect(await present(".statusbar__error")).toBe(false);
    // The document's own undo took it, which is the undo the editor uses everywhere else.
    expect(await propertyOf(".toolbar__edit-undo", "disabled")).toBe(false);

    const destination = path.join(saveDir, "from-transcription.srt");
    await clickElement(toplevel, ".toolbar__file-save-copy");
    const chooser = await waitForChooser("Save a copy of the subtitle");
    await answerChooser(chooser, destination, "save a copy");
    focusWindow(toplevel.id);
    await waitFor(
      async () => (await textOf(".statusbar__message"))?.includes(destination) === true,
      {
        timeout: 20000,
        message: `the status line to report the file written at ${destination}`,
      },
    );
    // Its bytes are on disk now, so a document that had never been saved is not unsaved work.
    expect(await present(".statusbar__dirty")).toBe(false);

    saved = readFileSync(destination);
    expect(saved.toString("utf8")).toContain(CORRECTION);

    // Reopened from disk: the edit is there, which is the end of the milestone's own sentence.
    await clickElement(toplevel, ".toolbar__file-open-subtitle");
    const reopen = await waitForChooser("Choose a subtitle");
    await answerChooser(reopen, destination, "subtitle");
    focusWindow(toplevel.id);
    await waitForSubtitleStatus(`SRT · ${cues.length} cues · LF`);
    expect(await rowText(1)).toBe(CORRECTION);
    expect(await present(".statusbar__dirty")).toBe(false);
  });

  it("asks before a transcription replaces unsaved work, and cancel keeps both", async () => {
    await editRow(toplevel, 2, SECOND_EDIT);
    expect(await present(".statusbar__dirty")).toBe(true);

    setStubMode("fast");
    await startRun(toplevel);
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "cancel");
    await waitForUnsavedDialogGone("Cancel");
    focusWindow(toplevel.id);

    // The document is untouched: both edits are still on screen and still unsaved.
    expect(await rowText(1)).toBe(CORRECTION);
    expect(await rowText(2)).toBe(SECOND_EDIT);
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await textOf(".statusbar__error")).toBe(null);
    // And so is the result: the run's cues are still listed, and still on offer.
    expect((await shownCues()).length).toBeGreaterThan(0);
    expect(await propertyOf(".asrbar__use", "tagName")).toBe("BUTTON");
    expect(readFileSync(path.join(saveDir, "from-transcription.srt")).equals(saved)).toBe(true);
  });

  it("takes the cues once the same question is answered with Discard", async () => {
    const cues = await shownCues();
    await clickElement(toplevel, ".asrbar__use");
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "discard");
    await waitForUnsavedDialogGone("Discard");
    focusWindow(toplevel.id);

    // Waited for on the offer going away, and not on the status line. The document being replaced
    // here is itself a transcription of the same run, so it has the same cue count and the same
    // format, and `SRT · N cues · LF` reads identical before and after: a guard that cannot tell
    // "not there" from "not there yet". The offer only goes when the result has been taken.
    await waitFor(
      async () => ((await propertyOf(".asrbar__use", "tagName")) === null ? true : null),
      {
        timeout: 20000,
        message: "the offer to go away, which is what taking the result does to it",
      },
    );
    expect(await textOf(".statusbar__document")).toContain(`SRT · ${cues.length} cues · LF`);
    expect(words(await rowText(1))).toBe(words(cues[0].text));
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await propertyOf(".toolbar__file-save", "disabled")).toBe(false);
    // Discarding drops edits that were never on disk; it never rewrites the file they came from.
    expect(readFileSync(path.join(saveDir, "from-transcription.srt")).equals(saved)).toBe(true);
  });

  it("shows progress, stays usable, and leaves nothing running when cancelled", async () => {
    setStubMode("slow");
    const pid = await startRun(toplevel);

    await waitForStatus("Transcribing");
    const percent = await waitFor(
      async () => {
        const value = await propertyOf(".asrbar__progress", "value");
        return typeof value === "number" && value > 0 ? value : null;
      },
      { timeout: 30000, message: "the progress bar to advance past zero" },
    );
    expect(percent).toBeGreaterThan(0);

    // The run is on a blocking task, so the window and the IPC layer are still answering: pausing
    // and playing the video is a round trip through both while whisper is going (CONTRIBUTING.md §7).
    const playLabel = await textOf(".controls__button");
    await clickElement(toplevel, ".controls__button");
    await waitFor(async () => (await textOf(".controls__button")) !== playLabel, {
      timeout: 15000,
      message: "the playback button to react while a transcription is running",
    });
    await clickElement(toplevel, ".controls__button");
    await waitFor(async () => (await textOf(".controls__button")) === playLabel, {
      timeout: 15000,
      message: "the playback button to come back to its first state",
    });

    expect(processLine(pid)).not.toBe(null);
    await clickElement(toplevel, ".asrbar__cancel");
    await waitForStatus("cancelled");

    // The acceptance criterion: no orphan. A child that was killed but never reaped is still here
    // as a zombie, so this fails on that too.
    const survivor = await waitFor(
      () => {
        const line = processLine(pid);
        return line === null ? "gone" : null;
      },
      { timeout: 15000, message: `the sidecar (pid ${pid}) to be gone: ${processLine(pid)}` },
    );
    expect(survivor).toBe("gone");
    expect(scratchRuns()).toEqual([]);

    // Back to a usable state: no error banner, and another run can be started.
    expect(await textOf(".asrbar__error")).toBe(null);
    expect(await propertyOf(".asrbar__start", "disabled")).toBe(false);
    expect(await propertyOf(".asrbar__cancel", "tagName")).toBe(null);
  });

  it("runs on the CPU when the GPU box is unticked", async () => {
    await clickElement(toplevel, ".asrbar__gpu");
    await waitFor(async () => (await propertyOf(".asrbar__gpu", "checked")) === false, {
      timeout: 10000,
      message: "the GPU checkbox to come unticked",
    });

    setStubMode("fast");
    await startRun(toplevel);
    const status = await waitForStatus(" cues");
    expect(status).toContain("CPU");
    expect(status).not.toContain("GPU");

    // The cues on screen are unsaved work, so this run asks before taking their place (M3.5).
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "discard");
    await waitForUnsavedDialogGone("Discard");
    focusWindow(toplevel.id);

    // -ng is what makes a run stay off the GPU inside a Vulkan build, so the CPU path is not a
    // label on the screen: it is on the command line.
    expect(stubArgv()).toContain("-ng");
    expect(await textOf(".asrbar__error")).toBe(null);
  });

  // Decision 24, B2: the document on screen is the transcription the CPU run left, and it has
  // never had a file. These four drive its first save through the window.
  it("asks a document with no file where it goes, and cancelling writes nothing", async () => {
    expect(await present(".statusbar__dirty")).toBe(true);

    await clickElement(toplevel, ".toolbar__file-save");
    const chooser = await waitForChooser(FIRST_SAVE_TITLE);
    await cancelChooser(chooser, "first save");
    focusWindow(toplevel.id);

    // Nothing written, nothing lost, and the question can be asked again.
    expect(readdirSync(firstSaveDir)).toEqual([]);
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await textOf(".statusbar__error")).toBe(null);
    expect(await propertyOf(".toolbar__file-save", "disabled")).toBe(false);
  });

  it("writes it where the chooser was answered, and it is not unsaved work any more", async () => {
    adopted = path.join(firstSaveDir, "first-save.srt");
    const firstCue = await rowText(1);
    expect(words(firstCue).length).toBeGreaterThan(0);

    await clickElement(toplevel, ".toolbar__file-save");
    const chooser = await waitForChooser(FIRST_SAVE_TITLE);
    await answerChooser(chooser, adopted, "first save");
    focusWindow(toplevel.id);
    await waitFor(async () => (await textOf(".statusbar__message"))?.includes(adopted) === true, {
      timeout: 20000,
      message: `the status line to report the file written at ${adopted}`,
    });

    expect(await present(".statusbar__dirty")).toBe(false);
    // Saved, not "saved a copy to": this file is the document's own from now on.
    expect(await textOf(".statusbar__message")).toContain(`Saved ${adopted}`);
    expect(readdirSync(firstSaveDir)).toEqual(["first-save.srt"]);
    expect(words(readFileSync(adopted, "utf8"))).toContain(words(firstCue));
  });

  it("writes to that same file on Ctrl+S afterwards, with no chooser", async () => {
    await editRow(toplevel, 1, AFTER_FIRST_SAVE);
    expect(await present(".statusbar__dirty")).toBe(true);

    focusWindow(toplevel.id);
    pressKey("ctrl+s");
    await waitFor(async () => ((await present(".statusbar__dirty")) === false ? true : null), {
      timeout: 20000,
      message: "the dirty marker to clear after Ctrl+S wrote the file the document adopted",
    });

    // Asked nobody where to go: it already knows, which is what adopting the path means.
    expect(findChooser(FIRST_SAVE_TITLE)).toBe(null);
    expect(readFileSync(adopted, "utf8")).toContain(AFTER_FIRST_SAVE);
    expect(readdirSync(firstSaveDir)).toEqual(["first-save.srt"]);
    expect(await textOf(".statusbar__error")).toBe(null);
  });

  it("holds the window open when the close gate's Save is asked and the chooser is cancelled", async () => {
    // A fresh transcription, so the document in the way has no file again. The one on screen was
    // just saved, so nothing is asked before it is replaced.
    setStubMode("fast");
    await startRun(toplevel);
    await waitForStatus(" cues");
    await waitFor(async () => ((await present(".statusbar__dirty")) === true ? true : null), {
      timeout: 30000,
      message: "the new transcription to become the open document",
    });
    const before = readdirSync(firstSaveDir);

    execFileSync("python3", [closeWindowTool, toplevel.id], { stdio: "inherit", timeout: 15000 });
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "save");
    await waitForUnsavedDialogGone("Save");

    // The gate's Save has nowhere to write, so it asks — and cancelling keeps the window and the
    // work in it (decision 17, decision 24 B2).
    const chooser = await waitForChooser(FIRST_SAVE_TITLE);
    await cancelChooser(chooser, "the close gate's first save");
    focusWindow(toplevel.id);

    expect(await present(".statusbar__dirty")).toBe(true);
    expect((await shownCues()).length).toBeGreaterThan(0);
    expect(readdirSync(firstSaveDir)).toEqual(before);
    expect(readFileSync(adopted, "utf8")).toContain(AFTER_FIRST_SAVE);
  });

  // BACKLOG.md M3.6: the third route. The document on screen is the transcription the run above
  // left, and it has never had a file either, so Save here has to ask the same question.
  it("asks where the work in the way goes, and cancelling that chooser replaces nothing", async () => {
    await editRow(toplevel, 1, IN_THE_WAY);
    expect(await present(".statusbar__dirty")).toBe(true);
    const before = readdirSync(firstSaveDir);

    setStubMode("fast");
    await startRun(toplevel);
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "save");
    await waitForUnsavedDialogGone("Save");

    const chooser = await waitForChooser(FIRST_SAVE_TITLE);
    await cancelChooser(chooser, "the transcription's first save");
    focusWindow(toplevel.id);

    // Nothing written, nothing replaced, and the work that was in the way is still on screen.
    expect(readdirSync(firstSaveDir)).toEqual(before);
    expect(await rowText(1)).toBe(IN_THE_WAY);
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await textOf(".statusbar__error")).toBe(null);
    // And the result is still there to be taken on a second answer.
    expect(await propertyOf(".asrbar__use", "tagName")).toBe("BUTTON");
  });

  it("writes it where that chooser is answered, and then takes the new cues", async () => {
    const replaced = path.join(firstSaveDir, "replaced.srt");
    const cues = await shownCues();

    await clickElement(toplevel, ".asrbar__use");
    const dialog = await waitForUnsavedDialog();
    answerDialog(dialog, "save");
    await waitForUnsavedDialogGone("Save");
    const chooser = await waitForChooser(FIRST_SAVE_TITLE);
    await answerChooser(chooser, replaced, "the transcription's first save");
    focusWindow(toplevel.id);

    // The replacement is the signal: the row the work in the way held is not that text any more.
    await waitFor(async () => ((await rowText(1)) === IN_THE_WAY ? null : true), {
      timeout: 20000,
      message: "the transcription to take the place of the document that was saved",
    });
    await waitForSubtitleStatus(`SRT · ${cues.length} cues · LF`);

    // The work that was in the way is on disk, where the chooser was answered.
    expect(readFileSync(replaced, "utf8")).toContain(IN_THE_WAY);
    // The new cues are the document, unsaved and with no file of their own.
    expect(words(await rowText(1))).toBe(words(cues[0].text));
    expect(await present(".statusbar__dirty")).toBe(true);
    expect(await propertyOf(".asrbar__use", "tagName")).toBe(null);
    expect(await textOf(".statusbar__error")).toBe(null);
    // The file an earlier first save adopted was not touched by any of this.
    expect(readFileSync(adopted, "utf8")).toContain(AFTER_FIRST_SAVE);
  });

  // T4's criterion in the state it is written for: a video open, nothing asked for, and no
  // transcription control anywhere. The run outlives the panel, so the same menu route brings back
  // the cues that were on screen before it closed.
  it("leaves nothing on screen when it is closed, with the video still open", async () => {
    expect(await present(".stage__empty")).toBe(false);
    const cues = await shownCues();
    expect(cues.length).toBeGreaterThan(0);

    await clickElement(toplevel, ".asrpanel__close");
    await waitFor(async () => ((await present(".asrpanel")) === false ? true : null), {
      timeout: 15000,
      message: "the transcription panel to close",
    });

    expect(await propertyOf(".asrbar__model", "tagName")).toBe(null);
    expect(await propertyOf(".asrbar__start", "tagName")).toBe(null);
    expect(await propertyOf(".asrbar__gpu", "tagName")).toBe(null);
    expect(await textOf(".asrbar__status")).toBe(null);
    expect(await shownCues()).toEqual([]);

    await openPanelFromMenu(toplevel);

    expect(await propertyOf(".asrbar__start", "tagName")).toBe("BUTTON");
    expect(await shownCues()).toEqual(cues);
  });

  it("refuses a damaged model and never hands it to the sidecar", async () => {
    // One bit, in place: the file keeps its catalogued length, so only its checksum tells it apart
    // from the model. See BACKLOG.md M3.2.
    const repair = damageModel();
    try {
      forgetStubRun();
      await clickElement(toplevel, ".asrbar__start");
      const banner = await waitFor(() => textOf(".asrbar__error"), {
        timeout: 30000,
        message: "the app to refuse the damaged model",
      });
      expect(banner).toContain("checksum");

      // The acceptance criterion: refused, never handed to whisper. Nothing was spawned, and no
      // audio was extracted for it either.
      expect(stubPid()).toBe(null);
      expect(scratchRuns()).toEqual([]);

      // And there is a way out: the row stops calling itself ready, and Download comes back.
      const label = await browser.execute(() => {
        const select = document.querySelector(".asrbar__model");
        return [...select.options].find((option) => option.value === select.value)?.text ?? "";
      });
      expect(label.toLowerCase()).toContain("damaged");
      expect(await propertyOf(".asrbar__download", "tagName")).toBe("BUTTON");
      expect(await propertyOf(".asrbar__start", "disabled")).toBe(true);
    } finally {
      repair();
    }
  });
});
