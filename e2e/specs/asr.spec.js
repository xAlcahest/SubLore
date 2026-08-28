/* global describe, it, before, document, window */
import { readdirSync, statSync } from "node:fs";
import path from "node:path";

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
import { clickAt, focusWindow, typeText } from "../lib/input.js";
import { repoRoot, requireVideoFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** What the status line says before anything has been transcribed. */
const IDLE_STATUS = "No transcription yet.";
/** A word tiny.en got right in the capture the stub sidecar replays. */
const HEARD_WORD = "terminology";
/** The fixture is 60 s of tone; every generated cue has to sit inside it. */
const FIXTURE_MS = 60000;

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

/** Start a run and wait until the sidecar has actually been spawned. */
async function startRun(toplevel) {
  forgetStubRun();
  await clickElement(toplevel, ".asrbar__start");
  return waitFor(() => stubPid(), {
    timeout: 30000,
    message: "the stand-in sidecar to be spawned and record its pid",
  });
}

describe("transcription", () => {
  let toplevel = null;
  let fixture = null;

  before(async () => {
    fixture = requireVideoFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(() => browser.execute(() => document.querySelector(".asrbar__model") !== null), {
      timeout: 30000,
      message: "the transcription bar to render",
    });
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
    await clickElement(toplevel, ".bar__input");
    await waitFor(() => browser.execute(() => document.activeElement?.className === "bar__input"), {
      timeout: 10000,
      message: "the video path field to take keyboard focus",
    });
    typeText(fixture);
    await clickElement(toplevel, ".bar__button");
    await waitFor(
      () =>
        browser.execute(
          () =>
            document.querySelector(".stage__empty") === null &&
            document.querySelector(".asrbar__start")?.disabled === false,
        ),
      { timeout: 30000, message: "the fixture to load and enable Transcribe" },
    );

    // CLAUDE.md §3.1: the user's media is read only. Snapshot what is beside it, and its own
    // metadata, and compare after the run.
    const videoDir = path.join(repoRoot, "fixtures", "video");
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
    // and playing the video is a round trip through both while whisper is going (CLAUDE.md §7).
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

    // -ng is what makes a run stay off the GPU inside a Vulkan build, so the CPU path is not a
    // label on the screen: it is on the command line.
    expect(stubArgv()).toContain("-ng");
    expect(await textOf(".asrbar__error")).toBe(null);
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
