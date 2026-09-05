/**
 * The ruler's arithmetic: which ticks a zoom puts along the top of the waveform, and what each one
 * is labelled.
 *
 * Kept out of the component because none of it touches a canvas and all of it is the reference's
 * own table: the tick bands (`src/audio_display.cpp:334-373`), the major-tick modulo, and the rule
 * that a label prints only what changed since the label before it (`src/audio_display.cpp:440-475`).
 */

/** One band of the tick scale: how long a minor tick is, how often a major one falls, how the
 * seconds are written. */
export type TickBand = {
  minorMs: number;
  majorEvery: number;
  decimals: number;
};

/**
 * The bands, chosen by how many pixels one second currently spans, deepest first. The thresholds
 * and the moduli are the reference's (`src/audio_display.cpp:337-370`); the ninth member its enum
 * declares is dead there, so there are eight.
 */
const BANDS: readonly { abovePxPerSecond: number; band: TickBand }[] = [
  { abovePxPerSecond: 3000, band: { minorMs: 1, majorEvery: 10, decimals: 2 } },
  { abovePxPerSecond: 300, band: { minorMs: 10, majorEvery: 10, decimals: 1 } },
  { abovePxPerSecond: 30, band: { minorMs: 100, majorEvery: 10, decimals: 0 } },
  { abovePxPerSecond: 3, band: { minorMs: 1000, majorEvery: 10, decimals: 0 } },
  { abovePxPerSecond: 1 / 3, band: { minorMs: 10_000, majorEvery: 6, decimals: 0 } },
  { abovePxPerSecond: 1 / 9, band: { minorMs: 60_000, majorEvery: 10, decimals: 0 } },
  { abovePxPerSecond: 1 / 90, band: { minorMs: 600_000, majorEvery: 6, decimals: 0 } },
];

/** Past the last threshold: one minor tick an hour. */
const HOURS: TickBand = { minorMs: 3_600_000, majorEvery: 10, decimals: 0 };

/** Only a media at least this long puts an hour in its labels (`src/audio_display.cpp:430`). */
const AN_HOUR_MS = 3_600_000;

export function bandFor(msPerPixel: number): TickBand {
  const pxPerSecond = 1000 / msPerPixel;
  return BANDS.find((step) => pxPerSecond > step.abovePxPerSecond)?.band ?? HOURS;
}

/** One mark on the ruler: where it is in device pixels, how tall it is drawn, what it says. */
export type RulerTick = { atPx: number; major: boolean; label: string | null };

function pad(value: number, width: number): string {
  return value.toString().padStart(width, "0");
}

/**
 * Every tick the window puts on the ruler, in order, with the labels the majors carry.
 *
 * `widthOf` measures a label in the font the ruler is actually drawn in, so the crowding rule is
 * decided against the drawn width rather than against a guess at it. A label that would start
 * before the previous one ends is dropped, tick and all, exactly as the reference drops it.
 */
export function rulerTicks(
  fromMs: number,
  msPerPixel: number,
  widthPx: number,
  durationMs: number,
  widthOf: (label: string) => number,
): RulerTick[] {
  if (!(msPerPixel > 0) || !(widthPx > 0)) {
    return [];
  }
  const band = bandFor(msPerPixel);
  const ticks: RulerTick[] = [];
  let index = Math.ceil(Math.max(0, fromMs) / band.minorMs);
  let lastRight = -1;
  // Under an hour the hour part never changes, so it is never printed.
  let lastHour = durationMs < AN_HOUR_MS ? 0 : -1;
  let lastMinute = -1;
  // At most one tick per device pixel can be told apart, so that is the bound on the walk.
  for (let step = 0; step <= widthPx; step += 1) {
    const ms = index * band.minorMs;
    const atPx = (ms - fromMs) / msPerPixel;
    if (atPx > widthPx) {
      break;
    }
    const major = index % band.majorEvery === 0;
    let label: string | null = null;
    if (major && atPx > lastRight) {
      const hour = Math.floor(ms / 3_600_000);
      const minute = Math.floor(ms / 60_000) % 60;
      const second = (ms - hour * 3_600_000 - minute * 60_000) / 1000;
      let text = "";
      if (hour !== lastHour) {
        text = `${hour}:${pad(minute, 2)}:`;
        lastHour = hour;
        lastMinute = minute;
      } else if (minute !== lastMinute) {
        text = `${minute}:`;
        lastMinute = minute;
      }
      text += band.decimals === 0 ? pad(Math.floor(second), 2) : second.toFixed(band.decimals);
      label = text;
      lastRight = atPx + widthOf(text);
    }
    ticks.push({ atPx, major, label });
    index += 1;
  }
  return ticks;
}
