/* global describe, it, before, console, document, window, getComputedStyle, Event, WheelEvent, performance */
/**
 * M2.4 W7, the half that moves on its own: the playhead between position events, and the window
 * following it.
 *
 * The playhead is found by its own colour in the canvas, so what is asserted is what was drawn.
 * Where the window sits is worked out by seeking to two known times while playback is stopped and
 * reading the two columns: the view does not follow while it is paused, so that calibration is
 * independent of the arithmetic it is used to check.
 */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import { requireWaveformFixture, windowHeight, windowWidth } from "../lib/paths.js";
import { waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

/** W7: the drawn head and the player's own figure, no further apart than one position event. */
const AGREEMENT_MS = 100;

/** The same budget a scroll step is held to, and in frames for the reason `editor.spec.js` gives. */
const TYPICAL_FRAMES = 4;
const WORST_FRAMES = 10;
const GIVE_UP_FRAMES = 120;

/** The column drawn in the accent, which is the playhead and nothing else on this canvas. */
function playheadColumn() {
  return browser.execute(() => {
    const canvas = document.querySelector(".waveform__canvas");
    if (canvas === null) {
      return null;
    }
    const accent = getComputedStyle(document.querySelector(".waveform"))
      .getPropertyValue("--accent")
      .trim();
    const want = (accent.match(/[0-9a-f]{2}/gi) ?? []).map((pair) => parseInt(pair, 16));
    if (want.length < 3) {
      return null;
    }
    const row = canvas.getContext("2d").getImageData(0, 0, canvas.width, 1).data;
    for (let x = 0; x < canvas.width; x += 1) {
      const i = x * 4;
      const near =
        Math.abs(row[i] - want[0]) +
        Math.abs(row[i + 1] - want[1]) +
        Math.abs(row[i + 2] - want[2]);
      if (near < 24) {
        return x;
      }
    }
    return -1;
  });
}

function position() {
  return browser.execute(() => Number(document.querySelector(".controls__slider")?.value ?? -1));
}

function transportLabel() {
  return browser.execute(() => document.querySelector(".controls__button")?.textContent ?? "");
}

function rectOf(selector) {
  return browser.execute((css) => {
    const element = document.querySelector(css);
    if (element === null) {
      return null;
    }
    const rect = element.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    return { x: rect.x * dpr, y: rect.y * dpr, width: rect.width * dpr, height: rect.height * dpr };
  }, selector);
}

async function clickElement(toplevel, selector) {
  const rect = await rectOf(selector);
  clickAt(toplevel.absX + rect.x + rect.width / 2, toplevel.absY + rect.y + rect.height / 2);
}

async function seekTo(seconds) {
  await browser.execute((target) => {
    const slider = document.querySelector(".controls__slider");
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    setter.call(slider, String(target));
    slider.dispatchEvent(new Event("input", { bubbles: true }));
    slider.dispatchEvent(new Event("change", { bubbles: true }));
  }, seconds);
  await browser.pause(200);
}

/** Two seeks while stopped give the window's left edge and its scale, in milliseconds per column. */
async function calibrate() {
  await seekTo(5);
  const first = await playheadColumn();
  await seekTo(25);
  const second = await playheadColumn();
  if (first < 0 || second < 0 || first === second) {
    throw new Error(`the playhead was not drawn at both calibration times: ${first} and ${second}`);
  }
  const msPerColumn = (25000 - 5000) / (second - first);
  return { fromMs: 5000 - first * msPerColumn, msPerColumn };
}

/** Steps of the zoom, in wheel notches with ctrl held. */
async function zoomIn(notches) {
  await wheelBy(-100 * notches, true);
}

/** Wheel over the canvas: negative is in or back, positive is out or along. */
async function wheelBy(deltaY, ctrl = false) {
  await browser.execute(
    (delta, withCtrl) => {
      const canvas = document.querySelector(".waveform__canvas");
      const box = canvas.getBoundingClientRect();
      for (let step = 0; step < Math.abs(delta) / 100; step += 1) {
        canvas.dispatchEvent(
          new WheelEvent("wheel", {
            bubbles: true,
            cancelable: true,
            clientX: box.x + box.width / 2,
            clientY: box.y + box.height / 2,
            deltaY: delta > 0 ? 100 : -100,
            ctrlKey: withCtrl,
          }),
        );
      }
    },
    deltaY,
    ctrl,
  );
  await browser.pause(150);
}

async function play(toplevel) {
  await clickElement(toplevel, ".controls__button");
  await waitFor(async () => ((await transportLabel()) === "Pause" ? true : null), {
    timeout: 15000,
    message: "the transport to read Pause, meaning playing",
  });
}

/** Read the playhead over a stretch of playback, so a claim is about a period and not an instant. */
async function sampleWhile(ms, check) {
  for (let taken = 0; taken < ms; taken += 250) {
    check(await playheadColumn());
    await browser.pause(250);
  }
}

describe("the waveform follows the playhead", () => {
  let toplevel = null;

  before(async () => {
    requireWaveformFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__open-video") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
    await clickElement(toplevel, ".toolbar__open-video");
    const chooser = await waitForChooser("Choose a video");
    await answerChooser(chooser, requireWaveformFixture(), "video");
    focusWindow(toplevel.id);
    await waitFor(() => browser.execute(() => document.querySelector(".waveform") !== null), {
      timeout: 30000,
      message: "the waveform panel to appear",
    });
  });

  it("draws the playhead where the seek put it, without waiting for a position event", async () => {
    const view = await calibrate();
    await seekTo(40);
    const column = await playheadColumn();
    const drawn = view.fromMs + column * view.msPerColumn;
    expect(Math.abs(drawn - 40000)).toBeLessThanOrEqual(view.msPerColumn);
  });

  it("carries the playhead across the panel while it plays, and agrees with the player", async () => {
    const view = await calibrate();
    await seekTo(10);
    const before = await playheadColumn();

    await clickElement(toplevel, ".controls__button");
    await waitFor(async () => ((await transportLabel()) === "Pause" ? true : null), {
      timeout: 15000,
      message: "the transport to read Pause, meaning playing",
    });
    await waitFor(async () => ((await position()) >= 20 ? true : null), {
      timeout: 30000,
      message: "playback to reach twenty seconds",
    });

    // Read as close together as the two can be read: the head is the interpolated figure and the
    // slider is the last event, and W7 allows them one event's worth of daylight.
    const column = await playheadColumn();
    const reported = await position();
    await clickElement(toplevel, ".controls__button");

    const drawn = view.fromMs + column * view.msPerColumn;
    console.log(
      `W7 head: drawn ${Math.round(drawn)} ms, player ${Math.round(reported * 1000)} ms, ` +
        `column ${column} of ${view.msPerColumn.toFixed(1)} ms each`,
    );
    expect(column).toBeGreaterThan(before);
    expect(Math.abs(drawn - reported * 1000)).toBeLessThanOrEqual(AGREEMENT_MS + view.msPerColumn);
  });
  it("stops following when a hand moves the view, and takes it back when playback restarts", async () => {
    // Deep enough that a few seconds fill the window, so following has something to do inside the
    // handful of seconds this test plays for.
    await zoomIn(6);
    await seekTo(10);
    await play(toplevel);

    // Following: the head stays on screen while the media runs past it.
    await sampleWhile(2000, (column) => expect(column).toBeGreaterThanOrEqual(0));

    // A hand on the wheel takes the view back a page, and nothing brings it forward again.
    await wheelBy(-600);
    await waitFor(async () => ((await playheadColumn()) === -1 ? true : null), {
      timeout: 15000,
      message: "the playhead to run off the window the hand left behind",
    });
    await sampleWhile(1000, (column) => expect(column).toBe(-1));

    // Playback starting takes it back.
    await clickElement(toplevel, ".controls__button");
    await waitFor(async () => ((await transportLabel()) === "Play" ? true : null), {
      timeout: 15000,
      message: "the transport to read Play, meaning stopped",
    });
    await play(toplevel);
    await waitFor(async () => ((await playheadColumn()) >= 0 ? true : null), {
      timeout: 15000,
      message: "the view to come back to the playhead",
    });
    await clickElement(toplevel, ".controls__button");
  });

  it("zooms and scrolls inside the frame budget", async () => {
    // Its own stage: from the whole file, so a zoom step has somewhere to go, and the scroll phase
    // starts deep enough that a notch has somewhere to go too. A step that draws the same row
    // again is a step against a bound, and this budget has nothing to say about those.
    await wheelBy(100 * 20, true);
    await browser.execute((giveUp) => {
      const canvas = document.querySelector(".waveform__canvas");
      const box = canvas.getBoundingClientRect();
      const notch = (deltaY, ctrlKey) =>
        canvas.dispatchEvent(
          new WheelEvent("wheel", {
            bubbles: true,
            cancelable: true,
            clientX: box.x + box.width / 2,
            clientY: box.y + box.height / 2,
            deltaY,
            ctrlKey,
          }),
        );
      const times = [];
      const row = () => {
        const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, 1).data;
        let out = "";
        for (let x = 0; x < canvas.width; x += 8) {
          out += data[x * 4] > 24 ? "1" : "0";
        }
        return out;
      };
      let done = 0;
      const runStep = () => {
        if (done >= 40) {
          window.__subloreView = times;
          return;
        }
        const before = row();
        const started = performance.now();
        // Twenty zoom steps, in and out by turns so every one of them crosses a level, then four
        // unmeasured ones to make room, then twenty scroll notches.
        if (done < 20) {
          notch(done % 2 === 0 ? -100 : 100, true);
        } else {
          if (done === 20) {
            // Two steps in and hard against the left edge. Two, not more: the fixture's blocks are
            // ten seconds long, and a window narrower than one of them sits inside a block that is
            // uniform, where a scroll moves the view and draws the same row — which would be
            // measured as a step that did nothing. At two steps the window is wider than a block,
            // so it always holds a boundary and every notch moves it.
            notch(-100, true);
            notch(-100, true);
            for (let back = 0; back < 40; back += 1) {
              notch(-100, false);
            }
          }
          // Fifty pixels, not a hundred: twenty notches of a hundred run off the end of a
          // minute-long file at this zoom, and a step against the end draws the same row again.
          notch(50, false);
        }
        const settle = (frames) => {
          const moved = row() !== before;
          if (moved || frames >= giveUp) {
            times.push({
              frames,
              ms: performance.now() - started,
              moved,
              kind: done < 20 ? "zoom" : "scroll",
            });
            done += 1;
            window.setTimeout(runStep, 0);
            return;
          }
          window.requestAnimationFrame(() => settle(frames + 1));
        };
        settle(0);
      };
      runStep();
    }, GIVE_UP_FRAMES);

    const times = await waitFor(() => browser.execute(() => window.__subloreView), {
      timeout: 60000,
      message: "twenty zoom steps and twenty scroll steps to finish",
    });
    const frames = times.map((step) => step.frames).sort((a, b) => a - b);
    const typical = frames[Math.floor(frames.length / 2)];
    const secondWorst = frames[frames.length - 2];
    console.log(
      `W7 view step: median ${typical} frames, second-worst ${secondWorst}, worst ` +
        `${frames[frames.length - 1]}, allowance ${TYPICAL_FRAMES} and ${WORST_FRAMES}. ` +
        `${times.map((step) => `${step.frames}f/${step.ms.toFixed(0)}ms${step.moved ? "" : "!"}`).join(" ")}`,
    );
    expect(times.length).toBe(40);
    // Every step drew something new: a run where they did not is a run that measured waiting.
    expect(times.filter((step) => step.moved).length).toBe(40);
    expect(typical).toBeLessThanOrEqual(TYPICAL_FRAMES);
    expect(secondWorst).toBeLessThanOrEqual(WORST_FRAMES);
  });
});
