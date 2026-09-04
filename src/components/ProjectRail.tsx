import { useState } from "react";

import { episodeLine, fileLine, fileName, type Project } from "../hooks/useProject";
import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import { type Command, type CommandId, type CommandRegistry } from "../types/chrome";
import { type EpisodeFileView, type EpisodeView, type FileRole } from "../types/project";
import RailDialog from "./RailDialog";
import RailMenu from "./RailMenu";

const ROLES: readonly FileRole[] = ["media", "source", "target"];

/** The node an open menu belongs to: what its commands act on, and what their greying is read from. */
type Target =
  | { kind: "project" }
  | { kind: "episode"; episode: EpisodeView }
  | { kind: "file"; episode: EpisodeView; file: EpisodeFileView };

type MenuAt = { x: number; y: number; target: Target };

/*
 * What each of the rail's three menus draws, ids only, and no list changes with the state: what
 * the state moves is the greying inside the records (CLAUDE.md, owner ruling 2026-09-03).
 */
const PROJECT_ITEMS: CommandId[] = [
  "project.create-project",
  // Second in both states, so opening a project is one route whether or not one is open.
  "project.open-project",
  "project.add-episode",
  "project.close-project",
  "project.delete-project",
];

const EPISODE_ITEMS: CommandId[] = [
  ...ROLES.map((role): CommandId => `project.attach-${role}`),
  "project.rename-episode",
  "project.delete-episode",
];

const FILE_ITEMS: CommandId[] = ["project.open-file", "project.locate-file", "project.detach-file"];

function itemsFor(target: Target): CommandId[] {
  if (target.kind === "project") {
    return PROJECT_ITEMS;
  }
  return target.kind === "episode" ? EPISODE_ITEMS : FILE_ITEMS;
}

type Ask = {
  title: string;
  message?: string;
  fieldLabel?: string;
  initial?: string;
  confirmLabel: string;
  onConfirm: (value: string) => void;
};

type ProjectRailProps = {
  project: Project;
  /** Activating an attached file opens it: a subtitle as the document, a video in the player. */
  onOpenFile: (file: EpisodeFileView) => void;
};

/**
 * The project rail: the series, its episodes and the files attached to them, as the tree the
 * mockup draws. Every command that changes anything is on the context menu (decision 24, A3), and
 * each of the five that v1 ships is confirmed once (D2).
 */
export default function ProjectRail({ project, onOpenFile }: ProjectRailProps) {
  const [menu, setMenu] = useState<MenuAt | null>(null);
  const [ask, setAsk] = useState<Ask | null>(null);

  const view = project.project;
  const episodes = view?.episodes ?? [];
  const selectedId = project.selected?.id ?? null;

  /** Run `act` on the path the chooser answers. A cancelled chooser does nothing at all. */
  async function withChosenPath(
    kind: "project-folder" | "project-file",
    act: (path: string) => void,
  ) {
    const path = await project.choosePath(kind);
    if (path !== null) {
      act(path);
    }
  }

  function projectCommands(): Command[] {
    return [
      {
        id: "project.create-project",
        label: en.project.menu.createProject,
        // A folder holds one project, so this is the command for a rail with none open; the three
        // below are the ones that need one.
        enabled: view === null,
        run: () => void withChosenPath("project-folder", (path) => void project.create(path)),
      },
      {
        id: "project.open-project",
        label: en.project.menu.openProject,
        enabled: true,
        run: () => void withChosenPath("project-folder", (path) => void project.open(path)),
      },
      {
        id: "project.add-episode",
        label: en.project.menu.addEpisode,
        enabled: view !== null,
        run: () =>
          setAsk({
            title: en.project.ask.addEpisodeTitle,
            fieldLabel: en.project.episodePlaceholder,
            confirmLabel: en.project.ask.addEpisodeConfirm,
            onConfirm: (value) => void project.addEpisode(value),
          }),
      },
      {
        id: "project.close-project",
        label: en.project.menu.closeProject,
        enabled: view !== null,
        run: () => {
          // Narrowed for the question, which names the project; `enabled` is what refuses it.
          if (view !== null) {
            setAsk({
              title: en.project.ask.closeProjectTitle,
              message: fill(en.project.ask.closeProjectMessage, { title: view.title }),
              confirmLabel: en.project.ask.closeProjectConfirm,
              onConfirm: () => void project.close(),
            });
          }
        },
      },
      {
        id: "project.delete-project",
        label: en.project.menu.deleteProject,
        enabled: view !== null,
        // The folder is named in the question, so nothing is ever deleted straight off a click.
        run: () => {
          if (view !== null) {
            setAsk({
              title: en.project.ask.deleteProjectTitle,
              message: fill(en.project.ask.deleteProjectMessage, { folder: view.folder }),
              confirmLabel: en.project.ask.deleteProjectConfirm,
              onConfirm: () => void project.remove(),
            });
          }
        },
      },
    ];
  }

  function episodeCommands(episode: EpisodeView): Command[] {
    const line = episodeLine(episode.ordinal, episode.title);
    return [
      ...ROLES.map((role): Command => ({
        id: `project.attach-${role}`,
        label: fill(en.project.menu.attach, { role: en.project.roles[role].toLowerCase() }),
        enabled: true,
        run: () =>
          void withChosenPath(
            "project-file",
            (path) => void project.attachFile(episode.id, role, path),
          ),
      })),
      {
        id: "project.rename-episode",
        label: en.project.menu.renameEpisode,
        enabled: true,
        run: () =>
          setAsk({
            title: en.project.ask.renameEpisodeTitle,
            fieldLabel: en.project.episodePlaceholder,
            initial: episode.title,
            confirmLabel: en.project.ask.renameEpisodeConfirm,
            onConfirm: (value) => void project.renameEpisode(episode.id, value),
          }),
      },
      {
        id: "project.delete-episode",
        label: en.project.menu.deleteEpisode,
        enabled: true,
        run: () =>
          setAsk({
            title: en.project.ask.deleteEpisodeTitle,
            message: fill(en.project.ask.deleteEpisodeMessage, { episode: line }),
            confirmLabel: en.project.ask.deleteEpisodeConfirm,
            onConfirm: () => void project.deleteEpisode(episode.id),
          }),
      },
    ];
  }

  function fileCommands(episode: EpisodeView, file: EpisodeFileView): Command[] {
    return [
      {
        id: "project.open-file",
        label: en.project.menu.openFile,
        // A record whose file is gone offers Locate instead, and the other is greyed rather than
        // taken away. Sublore never goes looking for it either way (decision 24, D3).
        enabled: !file.missing,
        run: () => onOpenFile(file),
      },
      {
        id: "project.locate-file",
        label: en.project.menu.locateFile,
        enabled: file.missing,
        run: () =>
          void withChosenPath("project-file", (path) => void project.locateFile(file.id, path)),
      },
      {
        id: "project.detach-file",
        label: en.project.menu.detachFile,
        enabled: true,
        run: () =>
          setAsk({
            title: en.project.ask.detachFileTitle,
            message: fill(en.project.ask.detachFileMessage, {
              name: fileName(file.path),
              episode: episodeLine(episode.ordinal, episode.title),
            }),
            confirmLabel: en.project.ask.detachFileConfirm,
            onConfirm: () => void project.detachFile(file.id),
          }),
      },
    ];
  }

  /**
   * The rail's commands, declared from the state this render holds and never polled, so no route
   * can draw a stale `enabled` (interface-spec 2.3). The node the menu belongs to is part of that
   * state: it is what an episode or file command acts on.
   */
  function registryFor(target: Target): CommandRegistry {
    let declared: Command[];
    if (target.kind === "project") {
      declared = projectCommands();
    } else if (target.kind === "episode") {
      declared = episodeCommands(target.episode);
    } else {
      declared = fileCommands(target.episode, target.file);
    }
    return Object.fromEntries(declared.map((command) => [command.id, command]));
  }

  /**
   * Opening an attached file is what a project is for (BACKLOG M4.5). A record whose file is gone
   * asks where it went instead, which is the same Locate its menu offers: nothing on disk is
   * searched, and the record is never dropped (decision 24, D3).
   */
  function activate(file: EpisodeFileView) {
    if (file.missing) {
      void withChosenPath("project-file", (path) => void project.locateFile(file.id, path));
      return;
    }
    onOpenFile(file);
  }

  function openMenuAt(event: React.MouseEvent, target: Target) {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ x: event.clientX, y: event.clientY, target });
  }

  /** Under the node it belongs to, which is where a click or a keystroke on the node wants it. */
  function openMenuUnder(element: HTMLElement, target: Target) {
    const box = element.getBoundingClientRect();
    setMenu({ x: box.left, y: box.bottom, target });
  }

  /** Shift+F10 and the menu key, so every command on the menu is reachable without a pointer. */
  function openMenuFromKeyboard(event: React.KeyboardEvent<HTMLElement>, target: Target) {
    if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) {
      return;
    }
    event.preventDefault();
    openMenuUnder(event.currentTarget, target);
  }

  return (
    <nav
      className="rail"
      aria-label={en.project.cap}
      onContextMenu={(event) => openMenuAt(event, { kind: "project" })}
    >
      <h2 className="rail__cap">{en.project.cap}</h2>

      {view === null ? (
        <button
          className="rail__empty"
          type="button"
          aria-haspopup="menu"
          onClick={(event) => openMenuUnder(event.currentTarget, { kind: "project" })}
          onKeyDown={(event) => openMenuFromKeyboard(event, { kind: "project" })}
        >
          {en.project.noProject}
        </button>
      ) : (
        <ul className="rail__tree">
          <li>
            {/* A plain click opens the project menu as well as a right-click: until File carries
                these commands, this node is where they live (decision 24, A3). */}
            <button
              className="rail__project"
              type="button"
              title={view.folder}
              aria-haspopup="menu"
              onClick={(event) => openMenuUnder(event.currentTarget, { kind: "project" })}
              onContextMenu={(event) => openMenuAt(event, { kind: "project" })}
              onKeyDown={(event) => openMenuFromKeyboard(event, { kind: "project" })}
            >
              {view.title}
            </button>
            {episodes.length === 0 ? (
              <p className="rail__none">{en.project.noEpisodes}</p>
            ) : (
              <ul className="rail__episodes">
                {episodes.map((episode) => (
                  <li key={episode.id}>
                    <button
                      className={
                        episode.id === selectedId
                          ? "rail__episode rail__episode--selected"
                          : "rail__episode"
                      }
                      type="button"
                      aria-current={episode.id === selectedId}
                      onClick={() => project.select(episode.id)}
                      onContextMenu={(event) => openMenuAt(event, { kind: "episode", episode })}
                      onKeyDown={(event) =>
                        openMenuFromKeyboard(event, { kind: "episode", episode })
                      }
                    >
                      {episodeLine(episode.ordinal, episode.title)}
                    </button>
                    {episode.files.length === 0 ? (
                      <p className="rail__none">{en.project.noFiles}</p>
                    ) : (
                      <ul className="rail__files">
                        {episode.files.map((file) => (
                          <li key={file.id}>
                            <button
                              className={
                                file.missing ? "rail__file rail__file--missing" : "rail__file"
                              }
                              type="button"
                              title={fileLine(file)}
                              onClick={() => activate(file)}
                              onContextMenu={(event) =>
                                openMenuAt(event, { kind: "file", episode, file })
                              }
                              onKeyDown={(event) =>
                                openMenuFromKeyboard(event, { kind: "file", episode, file })
                              }
                            >
                              <span className="rail__file-name">{fileName(file.path)}</span>
                              {file.missing && (
                                <span className="rail__missing">{en.project.missing}</span>
                              )}
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </li>
        </ul>
      )}

      {menu !== null && (
        <RailMenu
          x={menu.x}
          y={menu.y}
          label={en.project.menuLabel}
          items={itemsFor(menu.target)}
          commands={registryFor(menu.target)}
          onClose={() => setMenu(null)}
        />
      )}
      {ask !== null && (
        <RailDialog
          title={ask.title}
          message={ask.message}
          fieldLabel={ask.fieldLabel}
          initial={ask.initial}
          confirmLabel={ask.confirmLabel}
          onConfirm={(value) => {
            setAsk(null);
            ask.onConfirm(value);
          }}
          onCancel={() => setAsk(null)}
        />
      )}
    </nav>
  );
}
