import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLayer } from "../hooks/useLayers";
import { runCommand, type Command, type CommandId, type CommandRegistry } from "../types/chrome";

type RailMenuProps = {
  x: number;
  y: number;
  label: string;
  /** What this menu draws, as ids into `commands`, the way MenuBar takes its own (T3 C1). */
  items: CommandId[];
  commands: CommandRegistry;
  onClose: () => void;
};

/** The action half of an id, which is the class the harness names an item by (e2e/README.md). */
function itemToken(id: CommandId): string {
  return id.slice(id.indexOf(".") + 1);
}

/** This menu's items resolved from ids to the registry's records, as MenuBar resolves its own. */
function resolve(items: CommandId[], commands: CommandRegistry): Command[] {
  return items.map((id) => commands[id]);
}

/**
 * The rail's context menu. A layer in the sense `shell-layout.md` gives the word: it is painted
 * over the panels, it is transient, and it is dismissed rather than resized away.
 *
 * Episode-level commands live here rather than in the menu bar (decision 24, A3), and they come
 * from the registry like every other route (BACKLOG.md N18).
 */
export default function RailMenu({ x, y, label, items, commands, onClose }: RailMenuProps) {
  const list = useRef<HTMLUListElement>(null);
  const [at, setAt] = useState({ x, y });
  const drawn = resolve(items, commands);
  const first = drawn.findIndex((command) => command.enabled);

  // Mounted only while it is up, so the video surface hides for exactly that long (decision 1, T8).
  useLayer(true);

  // Opened from a pointer or from the keyboard, and either way the keyboard starts on the first
  // item that can run, as the menu bar's does.
  useEffect(() => {
    list.current?.querySelectorAll("button")[Math.max(first, 0)]?.focus();
  }, []);

  // A menu opened near an edge would hang off the window, and a command nobody can reach is a
  // command that is not there. Measured once it is laid out, and moved before it is painted.
  useLayoutEffect(() => {
    const box = list.current?.getBoundingClientRect();
    if (box === undefined) {
      return;
    }
    setAt({
      x: Math.max(0, Math.min(x, window.innerWidth - box.width)),
      y: Math.max(0, Math.min(y, window.innerHeight - box.height)),
    });
  }, [x, y]);

  /**
   * Every click ends at the one gate, greyed or not, which is what refuses it (interface-spec 2.3).
   * A greyed item is inert rather than dismissive: only a command that runs takes the menu down.
   */
  function activate(command: Command) {
    if (command.enabled) {
      onClose();
    }
    runCommand(commands, command.id);
  }

  function walk(event: React.KeyboardEvent<HTMLUListElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
      return;
    }
    event.preventDefault();
    const buttons = Array.from(list.current?.querySelectorAll("button") ?? []);
    const here = buttons.findIndex((button) => button === document.activeElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    const next = (here + step + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }

  return (
    <>
      {/* Anywhere else dismisses it, and swallows the click that dismissed it. */}
      <div className="railmenu__backdrop" onMouseDown={onClose} />
      <ul
        ref={list}
        className="railmenu"
        role="menu"
        aria-label={label}
        style={{ left: `${at.x}px`, top: `${at.y}px` }}
        onKeyDown={walk}
      >
        {drawn.map((command) => (
          <li key={command.id} role="none">
            <button
              className={`railmenu__item railmenu__item--${itemToken(command.id)}`}
              type="button"
              role="menuitem"
              // Greyed rather than disabled, so the click reaches the gate above instead of
              // stopping at the DOM (BACKLOG.md N18).
              aria-disabled={!command.enabled}
              onClick={() => activate(command)}
            >
              {command.label}
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}
