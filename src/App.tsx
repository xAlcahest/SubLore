import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { choosePath, type ChooseKind } from "./chooser";
import AboutDialog from "./components/AboutDialog";
import CueList from "./components/CueList";
import CurrentLine from "./components/CurrentLine";
import FindBar, { type FindMode } from "./components/FindBar";
import MenuBar from "./components/MenuBar";
import ModulePanel from "./components/ModulePanel";
import ModuleWorkBand from "./components/ModuleWorkBand";
import ProjectRail from "./components/ProjectRail";
import Sash from "./components/Sash";
import StatusBar from "./components/StatusBar";
import Toolbar from "./components/Toolbar";
import WaveBar, { type WaveBarButton } from "./components/WaveBar";
import Waveform, { type LiveTimes } from "./components/Waveform";
import TranscribePanel from "./components/TranscribePanel";
import VideoControls, { transportReadings } from "./components/VideoControls";
import VideoStage from "./components/VideoStage";
import { useAudioPeaks } from "./hooks/useAudioPeaks";
import { useCueSelection } from "./hooks/useCueSelection";
import { LayerContext, useLayerRegistry } from "./hooks/useLayers";
import { useAudioTracks } from "./hooks/useAudioTracks";
import { useLayout } from "./hooks/useLayout";
import { useWindowFloor } from "./hooks/useWindowFloor";
import { usePreview } from "./hooks/usePreview";
import { useContributions, type Contribution } from "./hooks/useContributions";
import { useModulePanels } from "./hooks/useModulePanels";
import { useModuleWork } from "./hooks/useModuleWork";
import { useModules, refusalLine } from "./hooks/useModules";
import { useSearch, type SearchOutcome } from "./hooks/useSearch";
import { useProject } from "./hooks/useProject";
import { useStartupFiles } from "./hooks/useStartupFiles";
import { useSubtitleFile, type RowsMoved } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer } from "./hooks/useVideoPlayer";
import { en } from "./i18n/en";
import { fill } from "./i18n/format";
import { commandFor, ownsTheKeyboard } from "./keyboard";
import { widestRow } from "./measure";
import { requestQuit } from "./quit";
import { replaceOne, type Match, type Query } from "./search";
import {
  runCommand,
  type Command,
  type CommandId,
  type CommandRegistry,
  type Menu,
} from "./types/chrome";
import { type EpisodeFileView } from "./types/project";
import { type CueRow } from "./types/subtitle";
import "./App.css";

/**
 * The registry key a contributed item gets: its module's position and the module's own id.
 *
 * Generated rather than written, which is what keeps the open repository free of any word a module
 * chose for itself (interface-spec 2.7, module-abi.md section 7).
 */
function moduleCommandId(item: Contribution): CommandId {
  return `module.${item.module}-${item.id}`;
}

/**
 * The panel a contributed item belongs to, or zero for one that belongs to none.
 *
 * A panel's own id is the primary row action, and an item whose parent is a panel is a secondary
 * one: both send that panel's id, and everything else sends zero (module-abi.md 4.1).
 */
function panelOf(item: Contribution, all: Contribution[]): number {
  if (item.kind === "panel") {
    return item.id;
  }
  const parent = all.find(
    (candidate) =>
      candidate.kind === "panel" &&
      candidate.module === item.module &&
      candidate.id === item.parent,
  );
  return parent?.id ?? 0;
}

/*
 * Every bound below was read off the rendered shell at 1024x700, never worked out on paper: W6 put
 * a ceiling under its own panel's default height that way and the panel could not be dragged back.
 *
 * Each is the number at 100 per cent, and each is taken against the interface size before it is
 * used (S2). Each was read again at 90 and at 150, in WebKitGTK because that is the engine the
 * app's webview is, and both ends are written beside it.
 *
 * Reading a bound once is not enough for a bound the fonts decide, which is why the video panel's
 * floor is no longer here: it is measured off the row it has to keep on one line, every time the
 * interface size or that row's copy changes. See `widestRow` in src/measure.ts.
 */

/**
 * A frozen contract with `MIN_WAVEFORM_HEIGHT` in src-tauri/src/layout.rs, which clamps the file.
 * The one bound with nothing to measure: it is the room the wave needs on either side of its middle
 * line, so it moves with the size to keep the panel's proportion, 58 at 90 and 96 at 150. Only at
 * 100 does it still equal the file guard, which does not scale, so a height left at 58 is stored as
 * 64 and opens there.
 */
const MIN_WAVEFORM_HEIGHT = 64;

/**
 * What the current line keeps: its times row and one line of text under it. Measured against the
 * layout at the smallest supported window, where the tools column is 216px and the default gives
 * the line 84 of them. A larger number here would put the ceiling under the default height and the
 * panel could never be dragged back to it.
 *
 * Read again: the line needs 60 at 90 per cent and 66 at 100, which the scaled bound covers. At 125
 * and 150 the times row wraps onto three rows in the column the default layout leaves and the line
 * needs 101 and 119, more than the bound. What is short there is the stored default height, which
 * is a pixel count that does not move with the interface; a floor tall enough to cover it would be
 * the W6 ceiling above.
 */
const MIN_CURRENT_LINE = 72;

/**
 * The narrowest tools column whose current line still fits the height the column gives it: at 176
 * the times row wraps onto three rows and the line needs the 84px it has, at 160 it wraps onto four
 * and needs 94. The fourth row arrives at 157 at 90 per cent and at 258 at 150, and the scaled
 * bound stops above both.
 */
const MIN_TOOLS_WIDTH = 176;

/**
 * The video column's own floor: the transport measures 46px and the stage is never smaller than its
 * own transport. What the tools column needs is taller than this and is measured below, live. A
 * frozen contract with `MIN_TOP_HEIGHT` in src-tauri/src/layout.rs, which clamps the file, and one
 * that holds at 100 only: the guard does not scale.
 *
 * The transport measures 41.6 at 90 per cent and 67 at 150, so twice it is 83.1 and 133.9 against a
 * scaled 82.8 and 138. The third of a pixel it falls short by at 90 is the transport's 1px border,
 * which is a pixel at every size.
 */
const MIN_TOP_HEIGHT = 92;

/**
 * The grid's header measured 25px and a row 28, so this is the header and three rows. The one bound
 * that does not scale whole: `ROW_HEIGHT` in CueList.tsx is a fixed 28 at every interface size and
 * only the header moves, measuring 23.2 at 90 per cent and 36 at 150, which puts the floor at 107.2
 * and 120. A scaled 109 would give 98 and 164, and 98 clips the third row, so the header alone is
 * scaled and it is not scaled downwards: at 90 the bound stays 109, two pixels over what fits.
 */
const MIN_GRID_HEIGHT = 109;

/** The part of the bound above that moves with the interface size. */
const MIN_GRID_HEAD = 25;

/**
 * How long a cue the user has just made lasts. A choice, not a derivation: an inserted cue has no
 * timing of its own yet, and two seconds is about what a subtitle line runs for.
 */
const NEW_CUE_MS = 2000;

/** How far lead-in pulls a start back and lead-out pushes an end on. The reference's own numbers
 * (`src/audio_timing_dialogue.cpp:551-561`), which are what a translator's ear is used to. */
const LEAD_IN_MS = 100;
const LEAD_OUT_MS = 350;

export default function App() {
  // Every HTML layer registers here while it is open, and the video surface hides for as long as
  // the set is not empty (decision 1, T8).
  const layers = useLayerRegistry();
  // The user's own expression never runs on this thread: it runs where it can be killed (F4a).
  const search = useSearch();
  // Read once at startup; the scan itself ran before this window existed (module-abi.md 3.5).
  const modules = useModules();
  const contributions = useContributions();
  const peaks = useAudioPeaks();
  const { layout, changeLayout, storeLayout } = useLayout();
  // The root declaration in shell.css reads this custom property; nothing else may set it (S1).
  // Before the paint, and before the effect below it, which measures type this size decides.
  useLayoutEffect(() => {
    if (layout !== null) {
      document.documentElement.style.setProperty(
        "--interface-scale",
        String(layout.interfaceScale),
      );
    }
  }, [layout?.interfaceScale]);
  // Decision 24 A4: View arrives with the first panel worth hiding. The choice lasts the session;
  // only the height outlives it (W6).
  const [waveformShown, setWaveformShown] = useState(true);
  // The audio region: the panel, its strip and the edge under them, drawn only when there are peaks
  // to draw and View has not turned them off. A panel with no provider takes no space. Declared
  // here because the effect that resolves the column's bounds is keyed on it.
  const audioPanelShown = waveformShown && peaks.filled > 0;
  const toolsRef = useRef<HTMLElement>(null);
  const topRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLElement>(null);
  // Null until the transport has been measured, which is before the first paint: a floor of zero
  // would be no floor at all, and a number here would be a floor read on one machine's fonts.
  const [transportFloor, setTransportFloor] = useState<number | null>(null);
  const [frame, setFrame] = useState({
    videoWidth: 0,
    toolsWidth: 0,
    toolsHeight: 0,
    lineHeight: 0,
    topWidth: 0,
    railWidth: 0,
    gridHeight: 0,
  });
  /** The panel's own centring, filled in by the panel: only it knows where its window is. */
  const centreOnCue = useRef<() => void>(() => {});
  /** The pair a hand is holding on the panel, so playing the selection plays where it is now. */
  const liveTimes = useRef<LiveTimes>(null);
  const { state, position, errorCode, open, togglePlayback, seek, playRange, setRegion } =
    useVideoPlayer(layers.covered);
  const audio = useAudioTracks(state.path, state.status === "ready");
  // The two states the grid indexes by row live below, so the patch that moves rows reaches them
  // through a box rather than directly: the document is read before the selection exists.
  const rowsMovedRef = useRef<RowsMoved>(() => {});
  const onRowsMoved = useCallback<RowsMoved>(
    (at, removed, inserted) => rowsMovedRef.current(at, removed, inserted),
    [],
  );
  // What a module's activation puts on screen, and how far it has got while it runs. Two states,
  // because a module may publish a table without doing anything long, and the reverse.
  const modulePanels = useModulePanels();
  const moduleWork = useModuleWork();
  const subtitle = useSubtitleFile(onRowsMoved, modulePanels.publish);
  const preview = usePreview();
  const project = useProject();
  // A finished transcription becomes the open document, and the backend asks about unsaved work on
  // the way there. See BACKLOG.md M3.5.
  const transcription = useTranscription((runId) => void adoptTranscription(runId));
  const ready = state.status === "ready";
  useStartupFiles(open, subtitle.open);
  // The cursor and the selection belong to the shell, not to the grid: the tools column edits
  // whichever row carries the cursor (decision 5, T5).
  const selection = useCueSelection(subtitle.cues.length, subtitle.openId);
  useEffect(() => {
    rowsMovedRef.current = selection.rowsMoved;
  }, [selection.rowsMoved]);

  // Every bound a sash is given comes from here rather than from a number: a fixed maximum would
  // clip a panel on a small window and waste room on a large one (W6). The current line is
  // re-resolved because the grid's document key remounts it.
  useEffect(() => {
    const column = toolsRef.current;
    const top = topRef.current;
    const grid = gridRef.current;
    if (column === null || top === null || grid === null) {
      return;
    }
    const line = column.querySelector(".currentline");
    const video = top.querySelector(".shell__video");
    // The one panel in the block whose width is fixed rather than dragged, and the one the window's
    // own floor has to leave room for beside the two that are.
    const rail = top.parentElement?.querySelector(".shell__rail") ?? null;
    // One snapshot, so the pairs a bound is worked out from always describe the same layout: a
    // width read a frame after its neighbour would let a drag walk past its own ceiling.
    const measure = () =>
      setFrame({
        videoWidth: video === null ? 0 : video.clientWidth,
        toolsWidth: column.clientWidth,
        toolsHeight: column.clientHeight,
        lineHeight: line === null ? 0 : line.clientHeight,
        topWidth: top.clientWidth,
        railWidth: rail === null ? 0 : rail.getBoundingClientRect().width,
        gridHeight: grid.clientHeight,
      });
    const observer = new ResizeObserver(measure);
    for (const element of [column, top, grid, line, video, rail]) {
      if (element !== null) {
        observer.observe(element);
      }
    }
    measure();
    return () => observer.disconnect();
  }, [subtitle.openId]);

  // Every bound above is a number at 100 per cent, so each is taken against the size the user
  // picked; 1 is what the fallback in tokens.css draws at, before the layout has been read (S2).
  const scale = layout?.interfaceScale ?? 1;
  const duration = state.duration ?? 0;

  // The video panel's floor: what the transport under it asks for on one row, taken off that row
  // rather than written down, because every width in it is one the machine's fonts decide (S2).
  // Before the paint, so the edge below is never drawn against a floor that has not been read yet.
  useLayoutEffect(() => {
    const controls = topRef.current?.querySelector<HTMLElement>(".controls") ?? null;
    if (controls === null) {
      return;
    }
    let live = true;
    const measure = () => {
      const width = widestRow(controls, transportReadings(duration));
      // Up to the whole pixel: the sum is in fractions of one, and the engine breaks a line in
      // sixty-fourths of one.
      if (live && width !== null) {
        setTransportFloor(Math.ceil(width));
      }
    };
    measure();
    // Type can arrive after the first paint, and every width above is the width of some type. The
    // promise has no failure: a page whose fonts never settle keeps the reading taken above.
    void document.fonts.ready.then(measure, () => {});
    return () => {
      live = false;
    };
  }, [scale, duration]);

  const minVideoWidth = transportFloor ?? 0;
  const minCurrentLine = MIN_CURRENT_LINE * scale;
  const minWaveformHeight = MIN_WAVEFORM_HEIGHT * scale;
  // The grid's three rows are a fixed 28px at every size, so only its header is scaled, and never
  // downwards: a floor short of a whole row clips it.
  const minGridHeight = MIN_GRID_HEIGHT + MIN_GRID_HEAD * Math.max(0, scale - 1);

  // The video panel is stored as a share of the row, so it keeps its proportion when the window
  // changes width; the sash works in pixels, which is what the row measures. Never under the floor:
  // a share stored at a wider window, or at a smaller interface size, opens below it.
  const videoWidth =
    layout === null ? 0 : Math.max(minVideoWidth, layout.videoFraction * frame.topWidth);
  const maxVideoWidth = Math.max(
    minVideoWidth,
    frame.videoWidth + frame.toolsWidth - MIN_TOOLS_WIDTH * scale,
  );
  const asFraction = (width: number) =>
    frame.topWidth > 0 ? width / frame.topWidth : (layout?.videoFraction ?? 0);

  // What the block under the chrome asks for, which is one of the widths the window's own floor is
  // the widest of: the rail, the two panels at their floors, and the edge between them. The gap
  // between the row and the two panels in it is that edge, read off the row rather than named.
  // Null until the video panel's floor is known, because a block floor short of it is not one.
  const minBlockWidth =
    transportFloor === null
      ? null
      : frame.railWidth +
        transportFloor +
        Math.max(0, frame.topWidth - frame.videoWidth - frame.toolsWidth) +
        MIN_TOOLS_WIDTH * scale;

  // How far the block may shrink before the column stops shrinking the current line and starts
  // pushing it out: the slack the line has over its own minimum, read off the rendered line.
  const minTopHeight = Math.max(
    MIN_TOP_HEIGHT * scale,
    frame.toolsHeight - Math.max(0, frame.lineHeight - minCurrentLine),
  );
  const maxTopHeight = Math.max(minTopHeight, frame.toolsHeight + frame.gridHeight - minGridHeight);
  const activeCue: CueRow | null =
    selection.active === null ? null : (subtitle.cues[selection.active] ?? null);
  // Saving writes the document, so it has to include the text sitting in either editor, and text
  // in an editor is unsaved work whether or not it has reached the document yet.
  const flushGrid = useRef<() => Promise<void>>(() => Promise.resolve());
  const flushLine = useRef<() => Promise<void>>(() => Promise.resolve());
  const [editorOpen, setEditorOpen] = useState(false);
  const [lineEdited, setLineEdited] = useState(false);
  /**
   * Where the caret last was in the current line's editor, and the row it was in. It outlives the
   * blur a menu click causes, which is why the split can still read it; a cursor on another row
   * leaves it unmatched and the split greyed.
   */
  const [caret, setCaret] = useState<{ index: number; offset: number } | null>(null);
  // The chooser is modal and answers on its own thread, so a second one asked for while it is up
  // would sit behind the first. Every chooser the chrome raises is raised here, so one flag covers
  // them all.
  const [choosing, setChoosing] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  // Absent until the menu asks for it, and gone again on Close: T4 takes the band off the screen.
  const [transcribeOpen, setTranscribeOpen] = useState(false);
  // The find band, and what it is looking for. The query outlives a close so reopening the band
  // offers the last search back, which is the cheap half of the reference's remembered list (F2).
  const [findMode, setFindMode] = useState<FindMode | null>(null);
  const [query, setQuery] = useState<Query>({ needle: "", matchCase: false, regex: false });
  const [replacement, setReplacement] = useState("");
  const [found, setFound] = useState<Match | null>(null);
  const [searched, setSearched] = useState(false);
  const [replaced, setReplaced] = useState<number | null>(null);
  /** What the last search refused, drawn in the band: a pattern that will not compile, or one the
   * engine never finished. Cleared by the next search. */
  const [refusal, setRefusal] = useState<"bad-pattern" | "slow" | null>(null);
  /** Whether the band stays inside the grid's selection. Off is the whole document (F4b). */
  const [inSelection, setInSelection] = useState(false);
  const [quitError, setQuitError] = useState<string | null>(null);

  async function pick(
    kind: ChooseKind,
    suggested: string | undefined,
    act: (path: string) => void,
  ) {
    if (choosing) {
      return;
    }
    setChoosing(true);
    try {
      const path = await choosePath(kind, suggested);
      // Cancelled is an outcome, not a failure: nothing opens, nothing is written, nothing is said.
      if (path !== null) {
        act(path);
      }
    } finally {
      setChoosing(false);
    }
  }

  /** Send whatever is sitting in either editor. Both are views of the document, so an operation
   * that reads the document flushes both or acts on one the user cannot see. See T5. */
  async function flushEditors() {
    await flushGrid.current();
    await flushLine.current();
  }

  /**
   * Save the document where it belongs. One that has never had a file is asked where that is, and
   * points at the path it is given from then on, so the next save writes there (decision 24, B2).
   */
  async function saveDocument() {
    await flushEditors();
    if (subtitle.summary === null) {
      return;
    }
    if (subtitle.summary.path === null) {
      await pick("subtitle-first-save", undefined, (path) => void subtitle.saveAs(path));
      return;
    }
    await subtitle.save();
  }

  /** A copy elsewhere, which leaves a document with a file of its own unsaved. */
  async function saveCopy() {
    await flushEditors();
    // The chooser opens on the open file's own name, which is what a copy is usually called; a
    // document that has never had a file has no name to offer.
    await pick("subtitle-save", subtitle.summary?.path ?? undefined, (path) => {
      void subtitle.saveAs(path);
    });
  }

  /**
   * Activating a file in the rail opens it, through the same commands the chooser route uses: a
   * video in the player, a subtitle as the document. See BACKLOG.md M4.5.
   */
  function openAttachedFile(file: EpisodeFileView) {
    if (file.role === "media") {
      void open(file.path);
      return;
    }
    void subtitle.open(file.path);
  }

  async function adoptTranscription(runId: number) {
    // Text sitting in an open editor is unsaved work too, so it reaches the document before
    // anything asks whether the document may be replaced.
    await flushEditors();
    await subtitle.adoptTranscription(runId);
  }

  /** Undo and redo take a step off the stack the user can see, so the text sitting in an editor
   * reaches the document first, exactly as a save does. See T5. */
  async function undoDocument() {
    await flushEditors();
    await subtitle.undo();
  }

  async function redoDocument() {
    await flushEditors();
    await subtitle.redo();
  }

  /**
   * A cue below the cursor, starting where that one ends and running `NEW_CUE_MS`. An empty
   * document takes its first cue at zero. See BACKLOG.md M2.7 E2.
   */
  async function insertCue() {
    await flushEditors();
    if (subtitle.summary === null) {
      return;
    }
    const at = selection.active;
    const previous = at === null ? null : (subtitle.cues[at] ?? null);
    const before = at === null || previous === null ? subtitle.cues.length : at + 1;
    const startMs = previous === null ? 0 : previous.endMs;
    await subtitle.insertCue(before, startMs, startMs + NEW_CUE_MS, "");
  }

  async function deleteCue() {
    await flushEditors();
    const at = selection.active;
    if (at === null || at >= subtitle.cues.length) {
      return;
    }
    await subtitle.deleteCue(at);
  }

  /**
   * Split at the caret in the current line's editor, at the playhead when the playhead is inside
   * the cue and at the cue's midpoint otherwise. Both halves are choices, not derivations: that
   * box holds the only caret in cue text there is, and a cue the playhead is not in has no moment
   * of its own to divide at. See BACKLOG.md M2.7 E3.
   */
  async function splitCue() {
    await flushEditors();
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (at === null || cue === null || caret === null || caret.index !== at) {
      return;
    }
    const playheadMs = Math.round(position * 1000);
    const inside = ready && playheadMs >= cue.startMs && playheadMs <= cue.endMs;
    await subtitle.splitCue(
      at,
      caret.offset,
      inside ? playheadMs : Math.floor((cue.startMs + cue.endMs) / 2),
    );
  }

  async function mergeCue() {
    await flushEditors();
    const at = selection.active;
    if (at === null || at + 1 >= subtitle.cues.length) {
      return;
    }
    await subtitle.mergeCue(at);
  }

  /**
   * The next match, from wherever the last one left off, with the cursor following it onto the cue
   * it lands in. A pattern in no cue moves nothing and says so on the band. See F2.
   */
  /**
   * The cues a search may look in: the selection when the band is held to it, and null for the
   * whole document. In file order, because the search walks it and wraps round it.
   */
  function scope(): number[] | null {
    return inSelection ? [...selection.selected].sort((first, second) => first - second) : null;
  }

  /** What every search reports back, wherever it was asked from. A refusal moves nothing. */
  function report(outcome: SearchOutcome): void {
    setReplaced(null);
    if (outcome.kind !== "found") {
      setRefusal(outcome.kind === "slow" ? "slow" : "bad-pattern");
      setSearched(false);
      setFound(null);
      return;
    }
    setRefusal(null);
    setSearched(true);
    setFound(outcome.match);
    if (outcome.match !== null) {
      // Inside the selection the cursor moves without collapsing it, or the next search would be
      // held to whatever this one landed on. It is the mode Ctrl and the arrows already use.
      selection.move(outcome.match.cue, inSelection ? "cursorOnly" : "plain");
    }
  }

  async function findFrom(at: Match | null): Promise<void> {
    report(await search.find(subtitle.cues, scope(), query, at));
  }

  function findNext() {
    void findFrom(found);
  }

  /**
   * Replace the match the band is standing on, then move to the next one. With no match in hand the
   * first press only finds, so a press never rewrites a cue the user has not been shown. That is
   * the two-press rule of interface-spec 9.2, expressed against the match this band holds rather
   * than against a text selection the grid does not have.
   */
  async function replaceCurrent() {
    const at = found;
    const cue = at === null ? null : (subtitle.cues[at.cue] ?? null);
    if (at === null || cue === null) {
      await findFrom(found);
      return;
    }
    const text = replaceOne(cue.text, at, query, replacement);
    await subtitle.setTexts([{ cue: at.cue, text }]);
    // Resume past what was just written, so a replacement containing the pattern is not re-found.
    // The length is the written one, which with regex on is not the replacement's own.
    const wrote = text.length - cue.text.length + (at.end - at.start);
    await findFrom({ cue: at.cue, start: at.start, end: at.start + wrote, found: at.found });
  }

  /** Every match in the document, in one edit and so in one undo step. See F1. */
  async function replaceAll() {
    const outcome = await search.replaceAll(subtitle.cues, scope(), query, replacement);
    if (outcome.kind !== "replaced") {
      report(outcome);
      return;
    }
    if (outcome.edits.length === 0) {
      setRefusal(null);
      setSearched(true);
      setFound(null);
      setReplaced(null);
      return;
    }
    await subtitle.setTexts(outcome.edits);
    setRefusal(null);
    setFound(null);
    setSearched(false);
    setReplaced(outcome.count);
  }

  /** Where the video is, to the millisecond the product reasons in (decision 11). */
  function playheadMs(): number {
    return Math.round(position * 1000);
  }

  /**
   * The cursor's cue takes the playhead as one of its boundaries. A start past its own end is
   * refused by the backend and the refusal reaches the status bar: a silent clamp would leave a cue
   * whose length nobody chose. See docs/playhead-tasks.md.
   */
  async function boundaryToPlayhead(which: "start" | "end") {
    await flushEditors();
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (at === null || cue === null) {
      return;
    }
    const now = playheadMs();
    await subtitle.setTimes(
      at,
      which === "start" ? now : cue.startMs,
      which === "end" ? now : cue.endMs,
    );
  }

  /** How much of the neighbourhood the two context commands play. The reference's own number. */
  const CONTEXT_MS = 500;

  /**
   * Play a stretch of the cursor's cue. mpv has no primitive for this, so the player holds a stop
   * target and the event thread pauses at it. See docs/play-range-tasks.md.
   */
  async function playCue(what: "line" | "before" | "after" | "first" | "last" | "to-end") {
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (cue === null) {
      return;
    }
    const start = cue.startMs / 1000;
    const end = cue.endMs / 1000;
    const context = CONTEXT_MS / 1000;
    // The player clamps into the file, so a cue near either edge asks for what it wants and gets
    // what exists rather than being special-cased twice.
    const range = {
      line: [start, end],
      before: [start - context, start],
      after: [end, end + context],
      // A line shorter than half a second is played whole rather than clipped to a window it does
      // not fill, which is what the reference does (`src/command/audio.cpp:306-320`).
      first: [start, Math.min(end, start + context)],
      last: [Math.max(start, end - context), end],
      "to-end": [start, state.duration ?? end],
    }[what];
    await playRange(range[0], range[1]);
  }

  /**
   * Play the selection, which is the current line's range as the panel has it right now: a boundary
   * a hand is dragging is played where the hand has put it, and Play line plays what the document
   * still holds. That difference is the whole reason the reference keeps both (interface-spec 5.9).
   */
  async function playSelection() {
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (cue === null) {
      return;
    }
    const live = liveTimes.current;
    await playRange((live?.startMs ?? cue.startMs) / 1000, (live?.endMs ?? cue.endMs) / 1000);
  }

  /** The step the boundaries move by. One size, no larger variant under Shift (owner ruling 23). */
  const NUDGE_MS = 10;

  /**
   * Move one boundary of the cursor's cue. A start pushed past its own end is refused by the
   * backend and the refusal reaches the status bar, the same as typing one in.
   */
  async function nudge(which: "start" | "end", by: number) {
    await flushEditors();
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (at === null || cue === null) {
      return;
    }
    // Never below zero: a boundary dragged off the front of the media is not a time.
    const moved = Math.max(0, (which === "start" ? cue.startMs : cue.endMs) + by);
    await subtitle.setTimes(
      at,
      which === "start" ? moved : cue.startMs,
      which === "end" ? moved : cue.endMs,
    );
  }

  /**
   * A boundary let go on the waveform. The pair travels in the one command that takes both, so a
   * whole drag is a single history entry. See docs/waveform-timing-tasks.md.
   */
  async function dragTimes(cue: number, startMs: number, endMs: number) {
    await flushEditors();
    await subtitle.setTimes(cue, startMs, endMs);
  }

  /** The video goes to one of the cursor's cue's boundaries. */
  async function videoToBoundary(which: "start" | "end") {
    const at = selection.active;
    const cue = at === null ? null : (subtitle.cues[at] ?? null);
    if (cue === null) {
      return;
    }
    await seek((which === "start" ? cue.startMs : cue.endMs) / 1000);
  }

  /**
   * The cursor goes to the cue the video is inside. In a gap it goes to the one that starts next,
   * because timing runs forwards, and past the last cue it does nothing.
   */
  function selectAtPlayhead() {
    const now = playheadMs();
    const covering = subtitle.cues.findIndex((cue) => now >= cue.startMs && now < cue.endMs);
    const target = covering >= 0 ? covering : subtitle.cues.findIndex((cue) => cue.startMs >= now);
    if (target >= 0) {
      selection.move(target, "plain");
    }
  }

  /** Quit through the one route the close gate guards, with the open editor flushed into the
   * document first so the gate is asked about it. See BACKLOG.md N6. */
  async function quit() {
    setQuitError(null);
    await flushEditors();
    try {
      await requestQuit();
    } catch {
      setQuitError(en.menu.errors.quitFailed);
    }
  }

  const dirty = subtitle.dirty || editorOpen || lineEdited;
  const blocked = subtitle.blockedPath !== null;
  // Whatever the stored layout says, and following until it says otherwise: the panel is decoration
  // on any file longer than its own window if it does not follow the line.
  const waveAutoscroll = layout?.waveAutoscroll ?? true;

  // S1: the View menu's five interface sizes, matching the Rust bounds in layout.rs.
  const interfaceScales = [
    { percent: 90, scale: 0.9 },
    { percent: 100, scale: 1 },
    { percent: 110, scale: 1.1 },
    { percent: 125, scale: 1.25 },
    { percent: 150, scale: 1.5 },
  ];
  /*
   * Every command the shell owns, written down once. `enabled` and `checked` are worked out here,
   * from the state this render already holds, so nothing polls them and no route can draw a stale
   * one (interface-spec 2.3). A generated set is entries like any other (interface-spec 2.7).
   */
  const declared: Command[] = [
    {
      id: "file.open-subtitle",
      label: en.menu.file.openSubtitle,
      accelerator: en.menu.keys.openSubtitle,
      enabled: !choosing,
      run: () => void pick("subtitle", undefined, (path) => void subtitle.open(path)),
    },
    {
      id: "video.open",
      label: en.menu.file.openVideo,
      accelerator: en.menu.keys.openVideo,
      enabled: !choosing && state.status !== "loading",
      run: () => void pick("video", undefined, (path) => void open(path)),
    },
    {
      id: "file.save",
      label: en.menu.file.save,
      accelerator: en.menu.keys.save,
      enabled: subtitle.summary !== null && dirty && !choosing,
      run: () => void saveDocument(),
    },
    {
      id: "file.save-copy",
      label: en.menu.file.saveCopy,
      accelerator: en.menu.keys.saveCopy,
      enabled: subtitle.summary !== null && !choosing,
      run: () => void saveCopy(),
    },
    {
      id: "file.discard",
      label: en.menu.file.discard,
      // Drawn always, usable only while an open was refused for unsaved edits: there is nothing to
      // discard the rest of the time, and `discardAndOpen` no-ops then anyway.
      enabled: blocked,
      run: () => void subtitle.discardAndOpen(),
    },
    {
      id: "app.quit",
      label: en.menu.file.quit,
      accelerator: en.menu.keys.quit,
      enabled: true,
      run: () => void quit(),
    },
    {
      id: "asr.transcribe",
      label: en.menu.edit.transcribe,
      enabled: !transcribeOpen,
      run: () => setTranscribeOpen(true),
    },
    {
      id: "edit.undo",
      label: en.menu.edit.undo,
      accelerator: en.menu.keys.undo,
      enabled: subtitle.canUndo,
      run: () => void undoDocument(),
    },
    {
      id: "edit.redo",
      label: en.menu.edit.redo,
      accelerator: en.menu.keys.redo,
      enabled: subtitle.canRedo,
      run: () => void redoDocument(),
    },
    {
      id: "edit.find",
      label: en.menu.edit.find,
      accelerator: en.menu.keys.find,
      // Nothing to search until a document is open, and the band is drawn greyed until then.
      enabled: subtitle.summary !== null,
      run: () => setFindMode("find"),
    },
    {
      id: "edit.find-next",
      label: en.menu.edit.findNext,
      accelerator: en.menu.keys.findNext,
      // Greyed with no pattern to step through: a find with nothing to find would put "No match"
      // under a field nobody has typed in yet, which is the accusation F2 refuses to make.
      enabled: subtitle.summary !== null && query.needle !== "",
      run: () => findNext(),
    },
    {
      id: "edit.replace",
      label: en.menu.edit.replace,
      accelerator: en.menu.keys.replace,
      // The same band in its other mode: the two are never open at once (interface-spec 9.2).
      enabled: subtitle.summary !== null,
      run: () => setFindMode("replace"),
    },
    {
      id: "time.start-to-playhead",
      label: en.menu.timing.startToPlayhead,
      accelerator: en.menu.keys.startToPlayhead,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void boundaryToPlayhead("start"),
    },
    {
      id: "time.end-to-playhead",
      label: en.menu.timing.endToPlayhead,
      accelerator: en.menu.keys.endToPlayhead,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void boundaryToPlayhead("end"),
    },
    {
      id: "video.to-cue-start",
      label: en.menu.timing.toCueStart,
      accelerator: en.menu.keys.videoToCueStart,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void videoToBoundary("start"),
    },
    {
      id: "video.to-cue-end",
      label: en.menu.timing.toCueEnd,
      accelerator: en.menu.keys.videoToCueEnd,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void videoToBoundary("end"),
    },
    {
      id: "time.start-earlier",
      label: en.menu.timing.startEarlier,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("start", -NUDGE_MS),
    },
    {
      id: "time.start-later",
      label: en.menu.timing.startLater,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("start", NUDGE_MS),
    },
    {
      id: "time.end-earlier",
      label: en.menu.timing.endEarlier,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("end", -NUDGE_MS),
    },
    {
      id: "time.end-later",
      label: en.menu.timing.endLater,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("end", NUDGE_MS),
    },
    {
      id: "time.prev-cue",
      label: en.menu.timing.prevCue,
      enabled: selection.active !== null && selection.active > 0,
      run: () => selection.move((selection.active ?? 0) - 1, "plain"),
    },
    {
      id: "time.next-cue",
      label: en.menu.timing.nextCue,
      enabled: selection.active !== null && selection.active < subtitle.cues.length - 1,
      run: () => selection.move((selection.active ?? 0) + 1, "plain"),
    },
    {
      id: "wave.play-selection",
      label: en.menu.timing.playSelection,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playSelection(),
    },
    {
      id: "time.play-line",
      label: en.menu.timing.playLine,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("line"),
    },
    {
      id: "wave.stop",
      label: en.menu.timing.stop,
      // Greyed unless something is playing, which is what the reference greys it on.
      enabled: ready && !state.paused,
      run: () => void togglePlayback(),
    },
    {
      id: "time.play-before",
      label: en.menu.timing.playBefore,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("before"),
    },
    {
      id: "time.play-after",
      label: en.menu.timing.playAfter,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("after"),
    },
    {
      id: "wave.play-first",
      label: en.menu.timing.playFirst,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("first"),
    },
    {
      id: "wave.play-last",
      label: en.menu.timing.playLast,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("last"),
    },
    {
      id: "time.play-to-end",
      label: en.menu.timing.playToEnd,
      enabled: subtitle.summary !== null && selection.active !== null && ready,
      run: () => void playCue("to-end"),
    },
    {
      id: "time.lead-in",
      label: en.menu.timing.leadIn,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("start", -LEAD_IN_MS),
    },
    {
      id: "time.lead-out",
      label: en.menu.timing.leadOut,
      enabled: subtitle.summary !== null && selection.active !== null,
      run: () => void nudge("end", LEAD_OUT_MS),
    },
    {
      // No cursor needed: finding the cue at the playhead is what gives it one.
      id: "edit.select-at-playhead",
      label: en.menu.timing.selectAtPlayhead,
      enabled: subtitle.summary !== null && subtitle.cues.length > 0 && ready,
      run: () => selectAtPlayhead(),
    },
    {
      id: "subtitle.insert",
      label: en.menu.subtitles.insert,
      // A document with no rows can still take its first one, so the cursor is not required here.
      enabled: subtitle.summary !== null,
      run: () => void insertCue(),
    },
    {
      id: "subtitle.delete",
      label: en.menu.subtitles.delete,
      enabled: activeCue !== null,
      run: () => void deleteCue(),
    },
    {
      id: "subtitle.split",
      label: en.menu.subtitles.split,
      // Only where a caret has been put, and only while the cursor is still on that row.
      enabled: activeCue !== null && caret !== null && caret.index === selection.active,
      run: () => void splitCue(),
    },
    {
      id: "subtitle.merge",
      label: en.menu.subtitles.merge,
      // The last row has nothing after it to join, which is the greying M2.7 E3 names.
      enabled: selection.active !== null && selection.active < subtitle.cues.length - 1,
      run: () => void mergeCue(),
    },
    {
      id: "help.about",
      label: en.menu.help.about,
      enabled: true,
      run: () => setAboutOpen(true),
    },
    {
      id: "video.toggle-subtitle-overlay",
      label: en.menu.view.subtitles,
      checked: preview.shown,
      // Enabled with no video and no document too, for the reason the waveform's toggle is: a
      // command that disappears when there is nothing to show reads as a command that is gone.
      enabled: true,
      run: () => preview.toggle(),
    },
    {
      id: "view.waveform-panel",
      label: en.menu.view.waveform,
      checked: waveformShown,
      // Enabled with no audio too: a toggle that disables itself when the thing it toggles is
      // absent tells the user the command is gone rather than that the panel has nothing to show.
      enabled: true,
      run: () => setWaveformShown((shown) => !shown),
    },
    {
      id: "wave.center-on-cue",
      label: en.menu.view.centreOnCue,
      // There is nothing to centre until the panel is on screen with a line drawn on it.
      enabled: audioPanelShown && activeCue !== null,
      run: () => centreOnCue.current(),
    },
    {
      id: "wave.toggle-autoscroll",
      label: en.menu.view.followCue,
      checked: waveAutoscroll,
      // Enabled with no audio too, for the reason the waveform's own toggle is: a command that
      // greys itself when there is nothing to show reads as a command that is gone.
      enabled: true,
      run: () => storeLayout({ waveAutoscroll: !waveAutoscroll }),
    },
    // Radio items, drawn the way the Audio menu draws its track list.
    ...interfaceScales.map(({ percent, scale }): Command => ({
      id: `view.interface-scale-${percent}`,
      label: fill(en.menu.view.scale, { percent }),
      checked: layout !== null && layout.interfaceScale === scale,
      group: "interface-scale",
      enabled: true,
      run: () => storeLayout({ interfaceScale: scale }),
    })),
    ...audio.tracks.map((track, index): Command => ({
      id: `audio.track.${track.id}`,
      label: track.title ?? track.lang ?? `${en.menu.audio.track} ${index + 1}`,
      checked: track.id === audio.currentId,
      group: "audio-track",
      // A single track is listed and cannot be switched away from: there is nowhere to go, and an
      // item that does nothing when clicked is worse than one that says so.
      enabled: audio.tracks.length > 1,
      run: () => audio.switchTo(track.id),
    })),
  ];
  /** The one map both draw routes read, so an item drawn anywhere has an entry here (T3 C1). */
  /**
   * A contributed item, as a command like any other.
   *
   * The core answers "is a document open", never "is this the module's thing", so `enableWhen` is
   * the whole of what it knows (module-abi.md 5.2). Running one carries the item's own id back to
   * the module that contributed it, with the cursor's row and nothing else.
   */
  function contributed(item: Contribution): Command {
    const enabled = (() => {
      switch (item.enableWhen) {
        case "always":
          return true;
        case "documentOpen":
          return subtitle.summary !== null;
        case "projectOpen":
          return project.project !== null;
        case "selectionNonEmpty":
          return selection.selected.size > 0;
      }
    })();
    const panelId = panelOf(item, contributions);
    return {
      id: moduleCommandId(item),
      label: item.label,
      enabled,
      // The module's own id and the state the gesture carried, and nothing about what it does.
      // The band comes down when this settles, which is the second of its two reasons to.
      run: (row) =>
        void subtitle
          .invokeModule(item.module, item.id, selection.active, panelId, row)
          .finally(moduleWork.clear),
    };
  }

  // A panel is a command too, because activating one of its rows activates the panel: it is the
  // primary row action, and it needs a registry record to be greyed and gated like any other.
  const contributedCommands = contributions
    .filter(
      (item) => item.kind === "menuItem" || item.kind === "menuTitle" || item.kind === "panel",
    )
    .map(contributed);

  const commands: CommandRegistry = Object.fromEntries(
    [...declared, ...contributedCommands].map((command) => [command.id, command]),
  );

  /*
   * The layout lists: ids only, one per route, and neither list changes with the state. What the
   * state moves is the greying inside the records above (CLAUDE.md, owner ruling 2026-09-03).
   */
  const menus: Menu[] = [
    {
      id: "file",
      title: en.menu.file.title,
      items: [
        "file.open-subtitle",
        "video.open",
        "file.save",
        "file.save-copy",
        "file.discard",
        "app.quit",
      ],
    },
    // Transcribe waits in Edit for an Audio title of its own, which arrives with the milestone that
    // registers those commands. File has no room: two keyboard routes pin its walk and its last item.
    {
      id: "edit",
      title: en.menu.edit.title,
      items: [
        "edit.undo",
        "edit.redo",
        "edit.find",
        "edit.find-next",
        "edit.replace",
        "asr.transcribe",
      ],
    },
    // Interface-spec 3 order: Subtitle sits right after Edit. Its fifth backend command,
    // subtitle_set_times, is not here: it is a field commit on the current line (T5), not an
    // action a menu item runs. See BACKLOG.md M2.7.
    {
      id: "subtitle",
      title: en.menu.subtitles.title,
      items: ["subtitle.insert", "subtitle.delete", "subtitle.split", "subtitle.merge"],
    },
    {
      id: "timing",
      title: en.menu.timing.title,
      // The order the panel's own strip runs in, so a translator who learns one has learned the
      // other (owner ruling 2026-09-05). The four nudges keep the end, where they have always been.
      items: [
        "time.prev-cue",
        "time.next-cue",
        "time.start-to-playhead",
        "time.end-to-playhead",
        "video.to-cue-start",
        "video.to-cue-end",
        "edit.select-at-playhead",
        "wave.play-selection",
        "time.play-line",
        "wave.stop",
        "time.play-before",
        "time.play-after",
        "wave.play-first",
        "wave.play-last",
        "time.play-to-end",
        "time.lead-in",
        "time.lead-out",
        "time.start-earlier",
        "time.start-later",
        "time.end-earlier",
        "time.end-later",
      ],
    },
    {
      id: "view",
      title: en.menu.view.title,
      items: [
        "video.toggle-subtitle-overlay",
        "view.waveform-panel",
        "wave.center-on-cue",
        "wave.toggle-autoscroll",
        ...interfaceScales.map(({ percent }): CommandId => `view.interface-scale-${percent}`),
      ],
    },
    // A media with no audio, or no media at all, leaves this with no items: the title is still on
    // the bar, greyed, because a title that comes and goes reads as a title that is gone.
    {
      id: "audio",
      title: en.menu.audio.title,
      items: audio.tracks.map((track): CommandId => `audio.track.${track.id}`),
    },
    { id: "help", title: en.menu.help.title, items: ["help.about"] },
    // A module's own titles, after the core's. A title exists exactly when a module pushed one with
    // children under it, so there is no branch anywhere that says a module is installed (5.1).
    ...contributions
      .filter((item) => item.kind === "menuTitle")
      .map((title) => ({
        id: `module-${title.module}-${title.id}`,
        title: title.label,
        items: contributions
          .filter(
            (item) =>
              item.kind === "menuItem" && item.module === title.module && item.parent === title.id,
          )
          .map(moduleCommandId),
      })),
  ];
  const toolbar: CommandId[][] = [
    ["file.open-subtitle", "video.open", "file.save", "file.save-copy", "file.discard"],
    ["edit.undo", "edit.redo"],
  ];

  /*
   * The waveform panel's own strip, left to right, with a divider between each group: the order the
   * reference draws it in, minus the positions §1's non-goals and decision 3 take out. Every entry
   * is a registry id, so a button here greys with the record the menu bar draws (T3 C1).
   */
  const waveBar: WaveBarButton[][] = [
    [
      { id: "time.prev-cue", short: en.wavebar.prevCue },
      { id: "time.next-cue", short: en.wavebar.nextCue },
      { id: "wave.play-selection", short: en.wavebar.playSelection },
      { id: "time.play-line", short: en.wavebar.playLine },
      { id: "wave.stop", short: en.wavebar.stop },
    ],
    [
      { id: "time.play-before", short: en.wavebar.playBefore },
      { id: "time.play-after", short: en.wavebar.playAfter },
      { id: "wave.play-first", short: en.wavebar.playFirst },
      { id: "wave.play-last", short: en.wavebar.playLast },
      { id: "time.play-to-end", short: en.wavebar.playToEnd },
    ],
    [
      { id: "time.lead-in", short: en.wavebar.leadIn },
      { id: "time.lead-out", short: en.wavebar.leadOut },
    ],
    [{ id: "wave.center-on-cue", short: en.wavebar.centreOnCue }],
    [{ id: "wave.toggle-autoscroll", short: en.wavebar.followCue }],
  ];

  // How narrow the window may be made, measured off the shell and carried to the window (S1). The
  // titles are what a module can add to, and the only part of either bar whose width is not the
  // same in every state: a greyed command is drawn, so nothing else here comes or goes.
  const windowFloor = useWindowFloor(
    scale,
    menus.map((menu) => menu.title).join("\n"),
    minBlockWidth,
  );

  // Read by the accelerator listener, which is registered once and outlives every render.
  const latest = useRef(commands);
  useEffect(() => {
    latest.current = commands;
  });

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      // A field keeps the chords a field owns, and every bare key but a function key. This is the
      // only listener that asks: the grid used to answer the same question again, for three keys.
      // Shift alone is not a modifier here, because Shift+A is still typing (F5).
      const chorded = event.ctrlKey || event.altKey || event.metaKey;
      if (ownsTheKeyboard(event.target, event.key.toLowerCase(), chorded)) {
        return;
      }
      const id = commandFor(latest.current, event);
      if (id === null) {
        return;
      }
      // The key is ours either way, so it never reaches the page; whether it runs is the gate's.
      event.preventDefault();
      runCommand(latest.current, id);
    };
    window.addEventListener("keydown", handle, true);
    return () => window.removeEventListener("keydown", handle, true);
  }, []);

  return (
    <LayerContext.Provider value={layers.registrar}>
      {/* The floor, published where it can be read back: it is a measurement and not a number, so
        nothing outside the app could work out what the window was told. */}
      <div
        className="shell"
        data-minimum-width={windowFloor.width ?? undefined}
        data-floor-seen={windowFloor.seen ?? undefined}
      >
        <header className="shell__chrome">
          <MenuBar menus={menus} commands={commands} />
          <Toolbar groups={toolbar} commands={commands} />
        </header>
        <div
          className="shell__body"
          style={layout === null ? undefined : { height: layout.topHeight }}
        >
          <aside className="shell__rail">
            <ProjectRail project={project} onOpenFile={openAttachedFile} />
          </aside>
          <div className="shell__top" ref={topRef}>
            <section
              className="shell__video"
              style={
                layout === null
                  ? undefined
                  : {
                      width: `${layout.videoFraction * 100}%`,
                      // The floor holds where the panel is drawn, not only where it may be dragged:
                      // a stored share opens the panel without a gesture anywhere near it.
                      minWidth: transportFloor ?? undefined,
                    }
              }
            >
              <VideoStage hasVideo={ready} onRegionChange={setRegion} />
              <VideoControls
                enabled={ready}
                paused={state.paused}
                duration={state.duration ?? 0}
                position={position}
                onToggle={() => void togglePlayback()}
                onSeek={(target) => void seek(target)}
              />
            </section>
            {/* The edge the owner asked for: the video gets bigger by dragging it (D1). It waits
              for the floor the same way it waits for the layout: an edge with no floor under it
              could be dragged to a width that has no transport. */}
            {layout !== null && transportFloor !== null && (
              <Sash
                axis="x"
                edge="video"
                size={videoWidth}
                min={minVideoWidth}
                max={maxVideoWidth}
                label={en.shell.videoSash}
                onResize={(width) => changeLayout({ videoFraction: asFraction(width) })}
                onRelease={(width) => storeLayout({ videoFraction: asFraction(width) })}
              />
            )}
            {/* The current line, with the waveform above it when there is one to draw (T5, W5). */}
            <section className="shell__tools" ref={toolsRef}>
              {/* Absent until the first chunk arrives, never an empty panel waiting for one, and
                absent again while View has it turned off. */}
              {/* Decision 24 E3: a media with no audio says so where the panel would be, in one
                line and not as a failure. */}
              {waveformShown && peaks.silent && (
                <p className="tools__silent">{en.waveform.noAudio}</p>
              )}
              {audioPanelShown && (
                <>
                  <Waveform
                    peaks={peaks}
                    positionMs={Math.round(position * 1000)}
                    durationMs={Math.round((state.duration ?? 0) * 1000)}
                    height={layout?.waveformHeight}
                    scale={scale}
                    paused={state.paused}
                    cueIndex={selection.active}
                    cue={activeCue}
                    cues={subtitle.cues}
                    selected={selection.selected}
                    autoscroll={waveAutoscroll}
                    centreRef={centreOnCue}
                    liveRef={liveTimes}
                    onDragTimes={(cue, startMs, endMs) => void dragTimes(cue, startMs, endMs)}
                    onSeek={(target) => void seek(target)}
                  >
                    {/* Inside the panel, under the wave: the stored height is the whole panel's,
                      the way the reference's is, so the strip moves with the edge. */}
                    <WaveBar groups={waveBar} commands={commands} />
                  </Waveform>
                  {layout !== null && (
                    <Sash
                      axis="y"
                      edge="waveform"
                      size={layout.waveformHeight}
                      min={minWaveformHeight}
                      max={Math.max(minWaveformHeight, frame.toolsHeight - minCurrentLine)}
                      label={en.waveform.sash}
                      onResize={(height) => changeLayout({ waveformHeight: height })}
                      onRelease={(height) => storeLayout({ waveformHeight: height })}
                    />
                  )}
                </>
              )}
              <CurrentLine
                key={subtitle.openId}
                index={selection.active}
                cue={activeCue}
                multiline={subtitle.summary?.format !== "ass"}
                flushRef={flushLine}
                onDraftChange={setLineEdited}
                onCaret={(offset) =>
                  setCaret(selection.active === null ? null : { index: selection.active, offset })
                }
                onCommit={subtitle.setText}
                onCommitTimes={subtitle.setTimes}
              />
            </section>
          </div>
        </div>
        {/* The edge between the whole top row and the grid below it (D1). */}
        {layout !== null && (
          <Sash
            axis="y"
            edge="grid"
            size={layout.topHeight}
            min={minTopHeight}
            max={maxTopHeight}
            label={en.shell.gridSash}
            onResize={(height) => changeLayout({ topHeight: height })}
            onRelease={(height) => storeLayout({ topHeight: height })}
          />
        )}
        {/* Full width, crossing under the rail: the layout drawing, the M2.0 criterion and T2 all
          say the grid takes everything below. Owner ruling 2026-09-02. */}
        <section className="shell__grid" ref={gridRef}>
          <CueList
            key={subtitle.openId}
            cues={subtitle.cues}
            selection={selection}
            multiline={subtitle.summary?.format !== "ass"}
            flushRef={flushGrid}
            onEditingChange={setEditorOpen}
            onCommit={subtitle.setText}
          />
        </section>
        {/* Under the grid, which is the one region that gives up space when it opens, so the top
          block keeps the height its sash left it at and the video surface does not move. See T4. */}
        {findMode !== null && (
          <FindBar
            mode={findMode}
            query={query}
            replacement={replacement}
            outcome={!searched ? "idle" : found === null ? "missing" : "found"}
            refusal={refusal}
            replaced={replaced}
            inSelection={inSelection}
            onInSelectionChange={(next) => {
              setInSelection(next);
              // The scope changed, so the match in hand is no longer where a search resumes from.
              setFound(null);
              setSearched(false);
              setReplaced(null);
              setRefusal(null);
            }}
            onQueryChange={(next) => {
              setQuery(next);
              // A changed pattern is a new search: resuming from the old match would skip the
              // first hit of the new one, and the last count is no longer about this pattern.
              setFound(null);
              setSearched(false);
              setReplaced(null);
              setRefusal(null);
            }}
            onReplacementChange={setReplacement}
            onFindNext={findNext}
            onReplace={() => void replaceCurrent()}
            onReplaceAll={() => void replaceAll()}
            onClose={() => setFindMode(null)}
          />
        )}
        {/* Beside the find band and the transcription one, in the region that gives up the
          space: a panel a module filled, and the line its work draws while it runs. */}
        {modulePanels.panels.map((panel) => {
          const contribution = contributions.find(
            (item) =>
              item.kind === "panel" && item.module === panel.module && item.id === panel.panelId,
          );
          if (contribution === undefined) {
            return null;
          }
          return (
            <ModulePanel
              key={`${panel.module}-${panel.panelId}`}
              panel={panel}
              title={contribution.label}
              primary={moduleCommandId(contribution)}
              actions={contributions
                .filter(
                  (item) =>
                    item.kind === "menuItem" &&
                    item.module === panel.module &&
                    item.parent === panel.panelId,
                )
                .map(moduleCommandId)}
              commands={commands}
              onClose={() => modulePanels.close(panel.module, panel.panelId)}
            />
          );
        })}
        {moduleWork.running && <ModuleWorkBand work={moduleWork} />}
        {transcribeOpen && (
          <TranscribePanel
            mediaPath={state.path}
            transcription={transcription}
            adoptedRunId={subtitle.adoptedRunId}
            onUse={(runId) => void adoptTranscription(runId)}
            onClose={() => setTranscribeOpen(false)}
          />
        )}
        <StatusBar
          summary={subtitle.summary}
          dirty={dirty}
          truncated={subtitle.truncated}
          saved={subtitle.saved}
          savedInPlace={subtitle.savedInPlace}
          subtitleError={subtitle.error}
          videoErrorCode={errorCode}
          projectDeleted={project.deleted}
          projectError={project.error}
          chromeError={quitError ?? (windowFloor.failed ? en.shell.errors.windowFloor : null)}
          waveformFailed={peaks.error !== null}
          previewFailed={preview.failed}
          moduleRefusals={modules.refused.map((refused) => refusalLine(refused, en.modules))}
        />
        {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      </div>
    </LayerContext.Provider>
  );
}
