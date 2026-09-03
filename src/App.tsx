import { useEffect, useRef, useState } from "react";

import { choosePath, type ChooseKind } from "./chooser";
import AboutDialog from "./components/AboutDialog";
import CueList from "./components/CueList";
import CurrentLine from "./components/CurrentLine";
import MenuBar from "./components/MenuBar";
import ProjectRail from "./components/ProjectRail";
import Sash from "./components/Sash";
import StatusBar from "./components/StatusBar";
import Toolbar from "./components/Toolbar";
import Waveform from "./components/Waveform";
import TranscribePanel from "./components/TranscribePanel";
import VideoControls from "./components/VideoControls";
import VideoStage from "./components/VideoStage";
import { useAudioPeaks } from "./hooks/useAudioPeaks";
import { useCueSelection } from "./hooks/useCueSelection";
import { LayerContext, useLayerRegistry } from "./hooks/useLayers";
import { useAudioTracks } from "./hooks/useAudioTracks";
import { useLayout } from "./hooks/useLayout";
import { usePreview } from "./hooks/usePreview";
import { useProject } from "./hooks/useProject";
import { useStartupFiles } from "./hooks/useStartupFiles";
import { useSubtitleFile } from "./hooks/useSubtitleFile";
import { useTranscription } from "./hooks/useTranscription";
import { useVideoPlayer } from "./hooks/useVideoPlayer";
import { en } from "./i18n/en";
import { fill } from "./i18n/format";
import { requestQuit } from "./quit";
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

/** The command an accelerator asks for, or null when the chrome does not own that key. */
function acceleratorFor(key: string, shift: boolean): CommandId | null {
  if (key === "o") {
    return shift ? "video.open" : "file.open-subtitle";
  }
  if (key === "s" && shift) {
    return "file.save-copy";
  }
  if (key === "q" && !shift) {
    return "app.quit";
  }
  return null;
}

/*
 * Every bound below was read off the rendered shell at 1024x700, never worked out on paper: W6 put
 * a ceiling under its own panel's default height that way and the panel could not be dragged back.
 *
 * Each is the number at 100 per cent, and each is taken against the interface size before it is
 * used (S2): at 150 the transport under the video is wider, so a fixed 220 stops keeping it on one
 * row. Each was read again at 90 and at 150, in WebKitGTK because that is the engine the app's
 * webview is, and both ends are written beside it. The instrument gave D1's own numbers back at
 * 100, which is what says it is the same measurement.
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
 * The narrowest video panel whose transport is still one row. Squeezed further the transport wraps
 * onto four rows and eats the picture, which is the sliver D1 refuses; measured by growing the
 * panel until `.controls` came back to its unwrapped height. It came back at 196 at 90 per cent and
 * at 326 at 150, and the scaled bound stops above both.
 */
const MIN_VIDEO_WIDTH = 220;

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

export default function App() {
  // Every HTML layer registers here while it is open, and the video surface hides for as long as
  // the set is not empty (decision 1, T8).
  const layers = useLayerRegistry();
  const peaks = useAudioPeaks();
  const { layout, changeLayout, storeLayout } = useLayout();
  // The root declaration in shell.css reads this custom property; nothing else may set it (S1).
  useEffect(() => {
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
  const toolsRef = useRef<HTMLElement>(null);
  const topRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLElement>(null);
  const [frame, setFrame] = useState({
    videoWidth: 0,
    toolsWidth: 0,
    toolsHeight: 0,
    lineHeight: 0,
    topWidth: 0,
    gridHeight: 0,
  });
  const { state, position, errorCode, open, togglePlayback, seek, setRegion } = useVideoPlayer(
    layers.covered,
  );
  const audio = useAudioTracks(state.path, state.status === "ready");
  const subtitle = useSubtitleFile();
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
    // One snapshot, so the pairs a bound is worked out from always describe the same layout: a
    // width read a frame after its neighbour would let a drag walk past its own ceiling.
    const measure = () =>
      setFrame({
        videoWidth: video === null ? 0 : video.clientWidth,
        toolsWidth: column.clientWidth,
        toolsHeight: column.clientHeight,
        lineHeight: line === null ? 0 : line.clientHeight,
        topWidth: top.clientWidth,
        gridHeight: grid.clientHeight,
      });
    const observer = new ResizeObserver(measure);
    for (const element of [column, top, grid, line, video]) {
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
  const minVideoWidth = MIN_VIDEO_WIDTH * scale;
  const minCurrentLine = MIN_CURRENT_LINE * scale;
  const minWaveformHeight = MIN_WAVEFORM_HEIGHT * scale;
  // The grid's three rows are a fixed 28px at every size, so only its header is scaled, and never
  // downwards: a floor short of a whole row clips it.
  const minGridHeight = MIN_GRID_HEIGHT + MIN_GRID_HEAD * Math.max(0, scale - 1);

  // The video panel is stored as a share of the row, so it keeps its proportion when the window
  // changes width; the sash works in pixels, which is what the row measures.
  const videoWidth = layout === null ? 0 : layout.videoFraction * frame.topWidth;
  const maxVideoWidth = Math.max(
    minVideoWidth,
    frame.videoWidth + frame.toolsWidth - MIN_TOOLS_WIDTH * scale,
  );
  const asFraction = (width: number) =>
    frame.topWidth > 0 ? width / frame.topWidth : (layout?.videoFraction ?? 0);

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
  // The chooser is modal and answers on its own thread, so a second one asked for while it is up
  // would sit behind the first. Every chooser the chrome raises is raised here, so one flag covers
  // them all.
  const [choosing, setChoosing] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  // Absent until the menu asks for it, and gone again on Close: T4 takes the band off the screen.
  const [transcribeOpen, setTranscribeOpen] = useState(false);
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
  const commands: CommandRegistry = Object.fromEntries(
    declared.map((command) => [command.id, command]),
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
      items: ["edit.undo", "edit.redo", "asr.transcribe"],
    },
    {
      id: "view",
      title: en.menu.view.title,
      items: [
        "video.toggle-subtitle-overlay",
        "view.waveform-panel",
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
  ];
  const toolbar: CommandId[][] = [
    ["file.open-subtitle", "video.open", "file.save", "file.save-copy", "file.discard"],
    ["edit.undo", "edit.redo"],
  ];

  // Read by the accelerator listener, which is registered once and outlives every render.
  const latest = useRef(commands);
  useEffect(() => {
    latest.current = commands;
  });

  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) {
        return;
      }
      // Ctrl+S, Ctrl+Z and Ctrl+Y are the cue list's: it flushes an open editor before it acts.
      const id = acceleratorFor(event.key.toLowerCase(), event.shiftKey);
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
      <div className="shell">
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
              style={layout === null ? undefined : { width: `${layout.videoFraction * 100}%` }}
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
            {/* The edge the owner asked for: the video gets bigger by dragging it (D1). */}
            {layout !== null && (
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
              {waveformShown && peaks.filled > 0 && (
                <>
                  <Waveform
                    peaks={peaks}
                    positionMs={Math.round(position * 1000)}
                    durationMs={Math.round((state.duration ?? 0) * 1000)}
                    height={layout?.waveformHeight}
                    paused={state.paused}
                  />
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
            onUndo={undoDocument}
            onRedo={redoDocument}
            onSave={saveDocument}
          />
        </section>
        {/* Under the grid, which is the one region that gives up space when it opens, so the top
          block keeps the height its sash left it at and the video surface does not move. See T4. */}
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
          chromeError={quitError}
          waveformFailed={peaks.error !== null}
          previewFailed={preview.failed}
        />
        {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      </div>
    </LayerContext.Provider>
  );
}
