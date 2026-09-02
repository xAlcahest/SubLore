import { useState } from "react";

import { episodeLine, fileLine, fileName, type Project } from "../hooks/useProject";
import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import { type EpisodeFileView, type EpisodeView, type FileRole } from "../types/project";
import RailDialog from "./RailDialog";
import RailMenu, { type RailMenuItem } from "./RailMenu";

const ROLES: readonly FileRole[] = ["media", "source", "target"];

type Menu = { x: number; y: number; items: RailMenuItem[] };

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
  const [menu, setMenu] = useState<Menu | null>(null);
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

  const openProject: RailMenuItem = {
    key: "open-project",
    label: en.project.menu.openProject,
    run: () => void withChosenPath("project-folder", (path) => void project.open(path)),
  };

  function projectItems(): RailMenuItem[] {
    if (view === null) {
      return [
        {
          key: "create-project",
          label: en.project.menu.createProject,
          run: () => void withChosenPath("project-folder", (path) => void project.create(path)),
        },
        openProject,
      ];
    }
    const folder = view.folder;
    const title = view.title;
    return [
      {
        key: "add-episode",
        label: en.project.menu.addEpisode,
        run: () =>
          setAsk({
            title: en.project.ask.addEpisodeTitle,
            fieldLabel: en.project.episodePlaceholder,
            confirmLabel: en.project.ask.addEpisodeConfirm,
            onConfirm: (value) => void project.addEpisode(value),
          }),
      },
      // Second in both states, so opening a project is one route whether or not one is open.
      openProject,
      {
        key: "close-project",
        label: en.project.menu.closeProject,
        run: () =>
          setAsk({
            title: en.project.ask.closeProjectTitle,
            message: fill(en.project.ask.closeProjectMessage, { title }),
            confirmLabel: en.project.ask.closeProjectConfirm,
            onConfirm: () => void project.close(),
          }),
      },
      {
        key: "delete-project",
        label: en.project.menu.deleteProject,
        // The folder is named in the question, so nothing is ever deleted straight off a click.
        run: () =>
          setAsk({
            title: en.project.ask.deleteProjectTitle,
            message: fill(en.project.ask.deleteProjectMessage, { folder }),
            confirmLabel: en.project.ask.deleteProjectConfirm,
            onConfirm: () => void project.remove(),
          }),
      },
    ];
  }

  function episodeItems(episode: EpisodeView): RailMenuItem[] {
    const line = episodeLine(episode.ordinal, episode.title);
    return [
      ...ROLES.map((role) => ({
        key: `attach-${role}`,
        label: fill(en.project.menu.attach, { role: en.project.roles[role].toLowerCase() }),
        run: () =>
          void withChosenPath(
            "project-file",
            (path) => void project.attachFile(episode.id, role, path),
          ),
      })),
      {
        key: "rename-episode",
        label: en.project.menu.renameEpisode,
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
        key: "delete-episode",
        label: en.project.menu.deleteEpisode,
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

  function fileItems(episode: EpisodeView, file: EpisodeFileView): RailMenuItem[] {
    // A record whose file is gone offers Locate instead of Open. Sublore never goes looking for it.
    const first: RailMenuItem = file.missing
      ? {
          key: "locate-file",
          label: en.project.menu.locateFile,
          run: () =>
            void withChosenPath("project-file", (path) => void project.locateFile(file.id, path)),
        }
      : {
          key: "open-file",
          label: en.project.menu.openFile,
          run: () => onOpenFile(file),
        };
    return [
      first,
      {
        key: "detach-file",
        label: en.project.menu.detachFile,
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

  function openMenuAt(event: React.MouseEvent, items: RailMenuItem[]) {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ x: event.clientX, y: event.clientY, items });
  }

  /** Under the node it belongs to, which is where a click or a keystroke on the node wants it. */
  function openMenuUnder(element: HTMLElement, items: RailMenuItem[]) {
    const box = element.getBoundingClientRect();
    setMenu({ x: box.left, y: box.bottom, items });
  }

  /** Shift+F10 and the menu key, so every command on the menu is reachable without a pointer. */
  function openMenuFromKeyboard(event: React.KeyboardEvent<HTMLElement>, items: RailMenuItem[]) {
    if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) {
      return;
    }
    event.preventDefault();
    openMenuUnder(event.currentTarget, items);
  }

  return (
    <nav
      className="rail"
      aria-label={en.project.cap}
      onContextMenu={(event) => openMenuAt(event, projectItems())}
    >
      <h2 className="rail__cap">{en.project.cap}</h2>

      {view === null ? (
        <button
          className="rail__empty"
          type="button"
          aria-haspopup="menu"
          onClick={(event) => openMenuUnder(event.currentTarget, projectItems())}
          onKeyDown={(event) => openMenuFromKeyboard(event, projectItems())}
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
              onClick={(event) => openMenuUnder(event.currentTarget, projectItems())}
              onContextMenu={(event) => openMenuAt(event, projectItems())}
              onKeyDown={(event) => openMenuFromKeyboard(event, projectItems())}
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
                      onContextMenu={(event) => openMenuAt(event, episodeItems(episode))}
                      onKeyDown={(event) => openMenuFromKeyboard(event, episodeItems(episode))}
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
                              onContextMenu={(event) => openMenuAt(event, fileItems(episode, file))}
                              onKeyDown={(event) =>
                                openMenuFromKeyboard(event, fileItems(episode, file))
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
          items={menu.items}
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
