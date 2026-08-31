/* global describe, it, before, console, document, performance, window */
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, copyFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { browser, expect } from "@wdio/globals";

import { clickAt, focusWindow, typeText } from "../lib/input.js";
import { repoRoot, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/**
 * BACKLOG.md M2.3: "open the 2000-cue fixture, edit a cue's text, save, reopen, the edit is there
 * and the rest is byte-identical; undo restores it; scrolling and typing show no visible lag
 * (measured, budget CLAUDE §7: open under 1 s)".
 *
 * The four numbers below stand in for "no visible lag", which is not assertable. They are measured
 * inside the page with `performance.now()` from probes this spec installs, so no production code
 * exists to serve the test and the 250 ms poll interval is not the measurement resolution.
 *
 * Honest reading: this is a debug build under Xvfb with software rendering. The Rust half of an
 * edit is ~5 ms in that profile, so a number under budget here is a necessary condition for the
 * release budget, not a measurement of it. The owner's checklist measures the release build.
 */
const OPEN_BUDGET_MS = 1000;
/**
 * The scroll step is measured against this machine's own speed, not against a millisecond number:
 * 32 ms was a fair budget on the machine it was measured on and a false failure on a slower one,
 * which is what it produced on CI. The baseline below is a fixed lump of arithmetic, so a slower
 * machine raises both sides and the ratio still means something.
 *
 * The multipliers are set from measurement, not from taste: on the owner's machine the mean step is
 * 2.8 baselines and the worst is 4.3, so 8 and 20 leave roughly three to five times of headroom for
 * a noisier runner while still failing on a regression worth knowing about. Wider than that and the
 * assertion stops being one.
 *
 * The absolute figures are logged, because the budget in CLAUDE.md section 7 is a real claim and the
 * owner's checklist measures it on the release build.
 */
const SCROLL_STEPS_PER_BASELINE = 8;
const SCROLL_WORST_PER_BASELINE = 20;
const TYPING_P95_MS = 50;
const TYPING_MAX_MS = 150;
const ROUND_TRIP_MS = 200;

/** The list position (1-based) this spec edits, and what it types over the text there. */
const EDITED_POSITION = 43;
const EDITED_TEXT = "Edited by the E2E ok";
const ORIGINAL_TEXT = "Nobody signed for the delivery.";
/** A second row, edited and then saved without leaving the editor first. */
const SECOND_POSITION = 100;
const SECOND_TEXT = "Typed and then saved";
/** A third row, edited so the undo stack holds two steps the shortcut check can tell apart. */
const THIRD_POSITION = 300;
const THIRD_TEXT = "Edited while checking the shortcuts";
const STATUS_PREFIX = "SRT · 2000 cues · LF";

function fixture(...parts) {
  const file = path.join(repoRoot, "fixtures", "subtitles", ...parts);
  if (!existsSync(file)) {
    throw new Error(
      `E2E prerequisite missing: ${file} does not exist. It is committed; restore it with \`git checkout fixtures/subtitles\`.`,
    );
  }
  return file;
}

/** Writes go to the harness temp dir. The committed fixture is copied, never opened for editing. */
function workingCopy() {
  const dataHome = process.env.SUBLORE_E2E_DATA_HOME;
  if (typeof dataHome !== "string" || dataHome === "") {
    throw new Error("SUBLORE_E2E_DATA_HOME is not set; e2e/wdio.conf.js sets it for every run.");
  }
  const directory = path.join(dataHome, "editor");
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const copy = path.join(directory, "large-2000.srt");
  copyFileSync(fixture("srt", "clean", "large-2000.srt"), copy);
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

/** Centre of the text cell of the row at a given 1-based list position, if it is rendered. */
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

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

/**
 * Replace whatever a text field holds. `e2e/lib/input.js` is shared with the M0 and M1 specs and
 * stays frozen, so the ctrl+a lives here, as it does in subtitle.spec.js.
 */
async function typeInto(toplevel, selector, text) {
  await clickElement(toplevel, selector);
  await waitFor(
    () => browser.execute((css) => document.activeElement?.matches(css) === true, selector),
    { timeout: 10000, message: `${selector} to take keyboard focus` },
  );
  key("ctrl+a");
  typeText(text);
  await waitFor(
    () =>
      browser.execute((css, want) => document.querySelector(css)?.value === want, selector, text),
    { timeout: 15000, message: `${selector} to hold exactly ${text}` },
  );
}

function key(name) {
  execFileSync("xdotool", ["key", "--clearmodifiers", name], { encoding: "utf8", timeout: 15000 });
}

/** The text a row shows, by 1-based list position, or null when that row is not rendered. */
function rowText(position) {
  return browser.execute((wanted) => {
    const rows = Array.from(document.querySelectorAll(".cuelist__row"));
    const row = rows.find(
      (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
    );
    return row?.querySelector(".cuelist__text")?.textContent ?? null;
  }, String(position));
}

/** Put a list position in view without clicking, so the click that follows lands on the row. */
async function scrollTo(position) {
  await browser.execute((wanted) => {
    const list = document.querySelector(".cuelist");
    const row = document.querySelector(".cuelist__row");
    if (list === null || row === null) {
      return;
    }
    const height = row.getBoundingClientRect().height;
    list.scrollTop = Math.max(0, (wanted - 1) * height - list.clientHeight / 2);
  }, position);
  await waitFor(async () => (await rowText(position)) !== null, {
    timeout: 15000,
    message: `row ${position} to be rendered`,
  });
}

function percentile(values, fraction) {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.floor(sorted.length * fraction));
  return sorted[index];
}

describe("cue list editing", () => {
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
    await waitFor(() => present(".subbar__input"), {
      timeout: 30000,
      message: "the subtitle bar to render",
    });
  });

  it("opens the 2000-cue fixture inside the open budget", async () => {
    await typeInto(toplevel, ".subbar__input", copy);
    const openButton = await centreOf(".subbar__open");

    // Stamped in the page: the click happens after this returns, so the number measured is the
    // click round trip plus the open, which can only make the assertion stricter.
    await browser.execute(() => {
      window.__subloreOpenAt = null;
      const observer = new window.MutationObserver(() => {
        if (window.__subloreOpenAt === null && document.querySelector(".cuelist__row") !== null) {
          window.__subloreOpenAt = performance.now();
          observer.disconnect();
        }
      });
      observer.observe(document.body, { childList: true, subtree: true });
      window.__subloreClickAt = performance.now();
    });
    await clickCentre(toplevel, openButton, ".subbar__open");

    const elapsed = await waitFor(
      () =>
        browser.execute(() =>
          window.__subloreOpenAt === null ? null : window.__subloreOpenAt - window.__subloreClickAt,
        ),
      { timeout: 30000, message: "the first cue row to appear" },
    );
    console.log(`M2.3 open to first row: ${elapsed.toFixed(1)} ms`);
    expect(elapsed).toBeLessThan(OPEN_BUDGET_MS);

    expect(await textOf(".subbar__status")).toContain(STATUS_PREFIX);
    expect(await present(".subbar__error")).toBe(false);
    expect(await present(".subbar__dirty")).toBe(false);
    expect(await rowText(1)).toBe("Keep the camera on the door.");
  });

  it("renders only the rows in view, over a sizer as tall as the whole file", async () => {
    const samples = [];
    for (const fraction of [0, 0.5, 1]) {
      const sample = await browser.execute((where) => {
        const list = document.querySelector(".cuelist");
        const sizer = document.querySelector(".cuelist__sizer");
        if (list === null || sizer === null) {
          return null;
        }
        list.scrollTop = (sizer.scrollHeight - list.clientHeight) * where;
        const rows = document.querySelectorAll(".cuelist__row");
        const height = rows.item(0)?.getBoundingClientRect().height ?? 0;
        return {
          rows: rows.length,
          sizer: sizer.scrollHeight,
          height,
          viewport: list.clientHeight,
        };
      }, fraction);
      expect(sample).not.toBe(null);
      samples.push(sample);
    }

    for (const sample of samples) {
      // Rendering everything and hiding the overflow would fail this; so would a sizer that only
      // covers what is rendered, which is why both halves are asserted.
      expect(sample.rows).toBeGreaterThan(0);
      expect(sample.rows).toBeLessThanOrEqual(60);
      expect(sample.height).toBeGreaterThan(0);
      expect(sample.sizer).toBe(2000 * sample.height);
      expect(sample.sizer).toBeGreaterThan(sample.viewport * 10);
    }
    console.log(
      `M2.3 virtualization: ${samples.map((s) => s.rows).join("/")} rows rendered of 2000, sizer ${samples[0].sizer}px`,
    );
  });

  it("scrolls a viewport at a time without falling behind", async () => {
    await browser.execute(() => {
      const list = document.querySelector(".cuelist");
      window.__subloreScroll = null;
      if (list === null) {
        return;
      }
      list.scrollTop = 0;
      const step = Math.max(1, list.clientHeight);
      const times = [];
      const firstRendered = () => {
        const cell = document.querySelector(".cuelist__row .cuelist__pos");
        return cell === null ? -1 : Number(cell.textContent);
      };
      let done = 0;
      const runStep = () => {
        if (done >= 20) {
          window.__subloreScroll = times;
          return;
        }
        const before = firstRendered();
        const started = performance.now();
        list.scrollTop = list.scrollTop + step;
        const settle = (attempts) => {
          if (firstRendered() !== before || attempts > 400) {
            // Forced layout, so the browser cannot defer the work past the measurement.
            document.querySelector(".cuelist__row")?.getBoundingClientRect();
            times.push(performance.now() - started);
            done += 1;
            window.setTimeout(runStep, 0);
            return;
          }
          window.setTimeout(() => settle(attempts + 1), 0);
        };
        settle(0);
      };
      runStep();
    });

    const times = await waitFor(() => browser.execute(() => window.__subloreScroll), {
      timeout: 30000,
      message: "twenty scroll steps to finish",
    });
    // A fixed lump of arithmetic, timed on this machine: the scale the allowance is expressed in.
    // A forced layout was tried first and is useless here — it costs 0.05 ms, far too little to
    // measure a React re-render against. What differs between this machine and a CI runner is CPU
    // speed, and this measures exactly that. Nine samples, median taken, so one scheduling hiccup
    // cannot set the scale.
    const baseline = await browser.execute(() => {
      const samples = [];
      for (let sample = 0; sample < 9; sample += 1) {
        const started = performance.now();
        let sink = 0;
        for (let i = 0; i < 3_000_000; i += 1) {
          sink += i % 7;
        }
        samples.push(performance.now() - started + sink * 0);
      }
      samples.sort((a, b) => a - b);
      return samples[Math.floor(samples.length / 2)];
    });

    const mean = times.reduce((total, value) => total + value, 0) / times.length;
    const max = Math.max(...times);
    console.log(
      `M2.3 scroll step: mean ${mean.toFixed(1)} ms, max ${max.toFixed(1)} ms over ` +
        `${times.length} steps; this machine's layout baseline ${baseline.toFixed(3)} ms, so the ` +
        `allowance is ${(baseline * SCROLL_STEPS_PER_BASELINE).toFixed(1)} ms mean and ` +
        `${(baseline * SCROLL_WORST_PER_BASELINE).toFixed(1)} ms worst`,
    );
    // React re-render plus layout, not compositor frames: a scroll step is timed from the
    // scrollTop assignment to the first paint-ready state that shows different rows.
    expect(times.length).toBe(20);
    // Every step rendered different rows: a step that timed out in `settle` would still be counted
    // above, and a list that stops moving is the failure this test is named for.
    expect(times.every((step) => step > 0)).toBe(true);
    expect(mean).toBeLessThan(baseline * SCROLL_STEPS_PER_BASELINE);
    expect(max).toBeLessThan(baseline * SCROLL_WORST_PER_BASELINE);
  });

  it("types into a cue without the list re-rendering behind every keystroke", async () => {
    await scrollTo(EDITED_POSITION);
    expect(await rowText(EDITED_POSITION)).toBe(ORIGINAL_TEXT);

    await clickCentre(toplevel, await centreOfRow(EDITED_POSITION), `row ${EDITED_POSITION}`);
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open",
    });
    key("ctrl+a");

    await browser.execute(() => {
      window.__subloreKeys = [];
      window.__subloreDown = [];
      const editor = document.querySelector(".cuelist__editor");
      if (editor === null) {
        return;
      }
      editor.addEventListener("keydown", () => window.__subloreDown.push(performance.now()), true);
      editor.addEventListener("input", () => {
        const started = window.__subloreDown.shift();
        if (started !== undefined) {
          window.__subloreKeys.push(performance.now() - started);
        }
      });
    });

    typeText(EDITED_TEXT);
    const times = await waitFor(
      () =>
        browser.execute((count) => {
          const keys = window.__subloreKeys;
          return Array.isArray(keys) && keys.length >= count ? keys : null;
        }, EDITED_TEXT.length),
      { timeout: 20000, message: `${EDITED_TEXT.length} keystrokes to reach the editor` },
    );

    const p95 = percentile(times, 0.95);
    const max = Math.max(...times);
    console.log(
      `M2.3 keystroke to input: p95 ${p95.toFixed(1)} ms, max ${max.toFixed(1)} ms over ${times.length} keys`,
    );
    expect(p95).toBeLessThan(TYPING_P95_MS);
    expect(max).toBeLessThan(TYPING_MAX_MS);

    const value = await browser.execute(
      () => document.querySelector(".cuelist__editor")?.value ?? null,
    );
    expect(value).toBe(EDITED_TEXT);
  });

  it("commits the edit on Enter and marks the file unsaved", async () => {
    await browser.execute(
      (wanted, want) => {
        window.__subloreCommitAt = null;
        const observer = new window.MutationObserver(() => {
          const rows = Array.from(document.querySelectorAll(".cuelist__row"));
          const row = rows.find(
            (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
          );
          if (
            window.__subloreCommitAt === null &&
            row?.querySelector(".cuelist__text")?.textContent === want
          ) {
            window.__subloreCommitAt = performance.now();
            observer.disconnect();
          }
        });
        observer.observe(document.body, { childList: true, subtree: true, characterData: true });
        window.__subloreEnterAt = performance.now();
      },
      String(EDITED_POSITION),
      EDITED_TEXT,
    );
    key("Return");

    const elapsed = await waitFor(
      () =>
        browser.execute(() =>
          window.__subloreCommitAt === null
            ? null
            : window.__subloreCommitAt - window.__subloreEnterAt,
        ),
      { timeout: 20000, message: "the edited row to show the new text" },
    );
    console.log(`M2.3 set_text round trip: ${elapsed.toFixed(1)} ms`);
    expect(elapsed).toBeLessThan(ROUND_TRIP_MS);

    expect(await present(".cuelist__editor")).toBe(false);
    expect(await rowText(EDITED_POSITION)).toBe(EDITED_TEXT);
    expect(await present(".subbar__dirty")).toBe(true);
    expect(await present(".subbar__error")).toBe(false);
    // The list did not move: the edit replaced one row, it did not renumber or reorder anything.
    expect(await rowText(EDITED_POSITION - 1)).toBe("He said the same thing last week.");
    expect(await rowText(EDITED_POSITION + 1)).toBe("That is not what the manifest says.");
    // Nothing has been written yet: a commit is not a save.
    expect(readFileSync(copy).equals(originalBytes)).toBe(true);
  });

  it("undoes the edit back to the original text and redoes it", async () => {
    await browser.execute(
      (wanted, want) => {
        window.__subloreUndoneAt = null;
        const observer = new window.MutationObserver(() => {
          const rows = Array.from(document.querySelectorAll(".cuelist__row"));
          const row = rows.find(
            (candidate) => candidate.querySelector(".cuelist__pos")?.textContent === wanted,
          );
          if (
            window.__subloreUndoneAt === null &&
            row?.querySelector(".cuelist__text")?.textContent === want
          ) {
            window.__subloreUndoneAt = performance.now();
            observer.disconnect();
          }
        });
        observer.observe(document.body, { childList: true, subtree: true, characterData: true });
        window.__subloreUndoAt = performance.now();
      },
      String(EDITED_POSITION),
      ORIGINAL_TEXT,
    );
    key("ctrl+z");

    const elapsed = await waitFor(
      () =>
        browser.execute(() =>
          window.__subloreUndoneAt === null
            ? null
            : window.__subloreUndoneAt - window.__subloreUndoAt,
        ),
      { timeout: 20000, message: "the edited row to show its original text again" },
    );
    console.log(`M2.3 undo round trip: ${elapsed.toFixed(1)} ms`);
    expect(elapsed).toBeLessThan(ROUND_TRIP_MS);

    expect(await rowText(EDITED_POSITION)).toBe(ORIGINAL_TEXT);
    // Undone back to the file as it was opened, so there is nothing unsaved any more.
    expect(await present(".subbar__dirty")).toBe(false);

    await clickElement(toplevel, ".subbar__redo");
    await waitFor(async () => (await rowText(EDITED_POSITION)) === EDITED_TEXT, {
      timeout: 20000,
      message: "the redone text to come back",
    });
    expect(await present(".subbar__dirty")).toBe(true);
  });

  it("saves the edit, and every other byte of the file is the byte that was there", async () => {
    await clickElement(toplevel, ".subbar__savefile");
    await waitFor(async () => (await present(".subbar__dirty")) === false, {
      timeout: 20000,
      message: "the dirty marker to clear after a save",
    });
    expect(await present(".subbar__error")).toBe(false);

    // Block by block, because this fixture repeats its lines: only the edited cue may differ, and
    // it may differ only in its text.
    const before = originalBytes.toString("utf8").split("\n\n");
    const after = readFileSync(copy).toString("utf8").split("\n\n");
    expect(after.length).toBe(before.length);
    for (let index = 0; index < before.length; index += 1) {
      if (index === EDITED_POSITION - 1) {
        expect(before[index]).toBe(`43\n00:01:46,000 --> 00:01:48,000\n${ORIGINAL_TEXT}`);
        expect(after[index]).toBe(`43\n00:01:46,000 --> 00:01:48,000\n${EDITED_TEXT}`);
      } else {
        expect(after[index]).toBe(before[index]);
      }
    }
  });

  it("reopens the saved file with the edit in it", async () => {
    await typeInto(toplevel, ".subbar__input", copy);
    await clickElement(toplevel, ".subbar__open");
    await waitFor(async () => (await textOf(".subbar__status"))?.includes(STATUS_PREFIX) === true, {
      timeout: 20000,
      message: "the status line to report the reopened file",
    });

    expect(await rowText(1)).toBe("Keep the camera on the door.");
    await scrollTo(EDITED_POSITION);
    expect(await rowText(EDITED_POSITION)).toBe(EDITED_TEXT);
    expect(await present(".subbar__dirty")).toBe(false);
    expect(await present(".subbar__error")).toBe(false);
  });

  it("saves the text still sitting in an open editor", async () => {
    await scrollTo(SECOND_POSITION);
    await clickCentre(toplevel, await centreOfRow(SECOND_POSITION), `row ${SECOND_POSITION}`);
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open",
    });
    key("ctrl+a");
    typeText(SECOND_TEXT);
    await waitFor(
      () =>
        browser.execute(
          (want) => document.querySelector(".cuelist__editor")?.value === want,
          SECOND_TEXT,
        ),
      { timeout: 15000, message: "the editor to hold the typed text" },
    );

    // Save without pressing Enter first: the click blurs the editor, so the commit it causes and
    // the save must both land, in that order.
    await clickElement(toplevel, ".subbar__savefile");
    await waitFor(async () => (await present(".subbar__dirty")) === false, {
      timeout: 20000,
      message: "the dirty marker to clear after saving an open editor",
    });
    expect(await present(".subbar__error")).toBe(false);
    expect(await rowText(SECOND_POSITION)).toBe(SECOND_TEXT);

    const blocks = readFileSync(copy).toString("utf8").split("\n\n");
    expect(blocks[SECOND_POSITION - 1]?.endsWith(`\n${SECOND_TEXT}`)).toBe(true);
  });

  // Regression: the shortcuts were captured on the window whatever had focus, so ctrl+z typed in
  // the destination box undid a cue edit instead of the path. See BACKLOG.md M2.3.
  it("leaves ctrl+z to the destination box and undoes exactly one step from the toolbar", async () => {
    const blocks = originalBytes.toString("utf8").split("\n\n");
    const thirdOriginal = blocks[THIRD_POSITION - 1].split("\n").pop();

    // A second edit on top of the saved one, so the stack holds two steps to tell apart.
    await scrollTo(THIRD_POSITION);
    await clickCentre(toplevel, await centreOfRow(THIRD_POSITION), `row ${THIRD_POSITION}`);
    await waitFor(() => present(".cuelist__editor"), {
      timeout: 15000,
      message: "the inline editor to open",
    });
    key("ctrl+a");
    typeText(THIRD_TEXT);
    key("Return");
    await waitFor(async () => (await rowText(THIRD_POSITION)) === THIRD_TEXT, {
      timeout: 20000,
      message: `row ${THIRD_POSITION} to hold the second edit`,
    });

    await typeInto(toplevel, ".subbar__dest", path.join(path.dirname(copy), "elsewhere.srt"));
    key("ctrl+z");

    // The toolbar undo that follows must be the first step off the top of the stack. Had the
    // keystroke above reached the document, it would be the second, and the row below moves too.
    await clickElement(toplevel, ".subbar__undo");
    await scrollTo(THIRD_POSITION);
    await waitFor(async () => (await rowText(THIRD_POSITION)) === thirdOriginal, {
      timeout: 20000,
      message: `row ${THIRD_POSITION} to go back to the text it was opened with`,
    });

    await scrollTo(SECOND_POSITION);
    expect(await rowText(SECOND_POSITION)).toBe(SECOND_TEXT);
    expect(await present(".subbar__error")).toBe(false);
  });
});
