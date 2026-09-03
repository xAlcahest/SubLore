/* global describe, it, before, document, window */
/**
 * M2.4 W9, decision 24 E3: a media with no audio opens normally, has no waveform panel, and one
 * line says so.
 *
 * The rule `shell-layout.md` states is that a panel with no provider takes no space. What this adds
 * is that the absence is explained rather than merely silent, and that the explanation is not a
 * failure: nothing here is an alert and nothing lands in the status bar.
 */
import { browser, expect } from "@wdio/globals";

import { answerChooser, waitForChooser } from "../lib/chooser.js";
import { clickAt, focusWindow } from "../lib/input.js";
import {
  requireSilentFixture,
  requireWaveformFixture,
  silentFixture,
  waveformFixture,
  windowHeight,
  windowWidth,
} from "../lib/paths.js";
import { ffmpegProcessesFor, waitFor } from "../lib/proc.js";
import { findToplevel } from "../lib/x11.js";

function present(selector) {
  return browser.execute((css) => document.querySelector(css) !== null, selector);
}

function textOf(selector) {
  return browser.execute((css) => document.querySelector(css)?.textContent ?? null, selector);
}

/** In CSS pixels, which is the unit the layout is written in and the height is stored in. */
function cssHeightOf(selector) {
  return browser.execute(
    (css) => document.querySelector(css)?.getBoundingClientRect().height ?? null,
    selector,
  );
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

async function openVideo(toplevel, fixture) {
  await clickElement(toplevel, ".toolbar__video-open");
  const chooser = await waitForChooser("Choose a video");
  await answerChooser(chooser, fixture, "video");
  focusWindow(toplevel.id);
}

describe("a media with no audio", () => {
  let toplevel = null;

  before(async () => {
    requireSilentFixture();
    requireWaveformFixture();
    toplevel = await waitFor(findToplevel, {
      timeout: 30000,
      message: `the ${windowWidth}x${windowHeight} "Sublore" toplevel to appear`,
    });
    focusWindow(toplevel.id);
    await waitFor(
      () => browser.execute(() => document.querySelector(".toolbar__video-open") !== null),
      { timeout: 30000, message: "the app UI to render" },
    );
  });

  it("opens, plays, and says where the panel would be that it has no audio", async () => {
    await openVideo(toplevel, silentFixture);

    await waitFor(() => present(".tools__silent"), {
      timeout: 30000,
      message: "the line about the media having no audio",
    });
    // The video itself opened: the transport knows how long it is.
    const duration = await browser.execute(() =>
      Number(document.querySelector(".controls__slider")?.max ?? 0),
    );
    expect(duration).toBeGreaterThan(0);

    expect(await present(".waveform")).toBe(false);
    // Named, not a bare `.sash`: D1 gave the shell two more edges and both are on screen here.
    expect(await present(".sash--waveform")).toBe(false);
    expect(await textOf(".tools__silent")).toBe(
      "This video has no audio, so there is no waveform to draw.",
    );
  });

  it("spawns no ffmpeg for it, and says nothing that reads as a failure", async () => {
    expect(ffmpegProcessesFor("waveform-silent")).toEqual([]);

    const alarming = await browser.execute(() =>
      Array.from(document.querySelectorAll('[role="alert"]')).map((node) => node.textContent),
    );
    expect(alarming).toEqual([]);
    expect(await present(".statusbar__waveform-error")).toBe(false);
  });

  it("offers nothing anywhere for attaching an audio file, which E3 rules out of v1", async () => {
    // Every label the shell draws, from both routes into every command plus the toolbar.
    const labels = await browser.execute(() =>
      Array.from(document.querySelectorAll(".menubar__title, .toolbar button, .menubar__item")).map(
        (node) => node.textContent ?? "",
      ),
    );
    const offered = labels.filter((label) => /attach|add audio|audio file|soundtrack/i.test(label));
    expect(offered).toEqual([]);
  });

  it("brings the panel back, at the height it was left at, for a media that has audio", async () => {
    // The height is read here rather than assumed: it lives in the app's own store and another spec
    // in this run may have dragged it. What the criterion asks is that the panel comes back where
    // it was, which is a question about this run and not about the default.
    await openVideo(toplevel, waveformFixture);
    await waitFor(() => present(".waveform"), {
      timeout: 30000,
      message: "the waveform panel for the media that has audio",
    });
    const before = await cssHeightOf(".waveform");
    expect(before).toBeGreaterThan(0);

    await openVideo(toplevel, silentFixture);
    await waitFor(() => present(".tools__silent"), {
      timeout: 30000,
      message: "the line about the media having no audio",
    });
    expect(await present(".waveform")).toBe(false);

    await openVideo(toplevel, waveformFixture);
    await waitFor(() => present(".waveform"), {
      timeout: 30000,
      message: "the waveform panel to come back",
    });
    expect(await present(".tools__silent")).toBe(false);
    expect(await cssHeightOf(".waveform")).toBe(before);
  });
});
