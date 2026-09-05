import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLayer } from "../hooks/useLayers";
import {
  commandToken,
  runCommand,
  type Command,
  type CommandId,
  type CommandRegistry,
  type Menu,
} from "../types/chrome";

type MenuBarProps = {
  menus: Menu[];
  commands: CommandRegistry;
};

/** A menu's items resolved from ids to the registry's records (T3 C1). */
function resolve(menu: Menu, commands: CommandRegistry): Command[] {
  return menu.items.map((id) => commands[id]);
}

/** The first item the cursor may sit on, or -1 when every item in the menu is disabled. */
function firstEnabled(items: Command[]): number {
  return items.findIndex((item) => item.enabled);
}

/** Whether a title has anything to open. A menu with no items is greyed rather than dropped (C2). */
function opens(menu: Menu): boolean {
  return menu.items.length > 0;
}

/**
 * The next title in `direction` that has something to open, or -1 past the end of the bar. The
 * walk stops at the ends rather than wrapping, which is what the bar has always done.
 */
function stepTitle(menus: Menu[], from: number, direction: number): number {
  for (let index = from + direction; index >= 0 && index < menus.length; index += direction) {
    if (opens(menus[index])) {
      return index;
    }
  }
  return -1;
}

/**
 * The next enabled item in `direction`, wrapping at the ends, or the one we are on when the menu
 * holds no other enabled item. From -1, where a mouse-opened menu starts, down lands on the first
 * enabled item and up on the last.
 */
function stepOver(items: Command[], from: number, direction: number): number {
  const count = items.length;
  for (let step = 1; step <= count; step += 1) {
    const index = (((from + direction * step) % count) + count) % count;
    if (items[index].enabled) {
      return index;
    }
  }
  return from;
}

/**
 * The menu bar: CSS chrome and not a native menu, so Windows and Linux draw the same thing
 * (decision 1). Alt opens the first title, arrows walk it skipping disabled items, Enter activates
 * and Escape closes and hands the keyboard back — the table in shell-layout.md.
 */
export default function MenuBar({ menus, commands }: MenuBarProps) {
  const [open, setOpen] = useState<number | null>(null);
  const [cursor, setCursor] = useState(-1);
  const barRef = useRef<HTMLDivElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  /** Where the keyboard was before Alt, so Escape can hand it back. */
  const restoreTo = useRef<HTMLElement | null>(null);
  /** Read by the window listeners, which are registered once and outlive every render. */
  const latest = useRef({ menus, commands, open, cursor });

  // The open dropdown is a layer and the bar itself is not, so the picture gets out of the way only
  // while one is down. Walking from one title to the next never lets it back (decision 1, T8).
  useLayer(open !== null);

  function openMenu(index: number, item: number) {
    if (restoreTo.current === null && document.activeElement instanceof HTMLElement) {
      restoreTo.current = document.activeElement;
    }
    setOpen(index);
    setCursor(item);
  }

  function closeMenu(giveFocusBack: boolean) {
    const previous = restoreTo.current;
    restoreTo.current = null;
    setOpen(null);
    setCursor(-1);
    if (giveFocusBack) {
      previous?.focus();
    }
  }

  /** The menu closes, then the one gated path decides whether anything runs (C3). */
  function activate(registry: CommandRegistry, id: CommandId) {
    closeMenu(true);
    runCommand(registry, id);
  }

  function onKeyDown(event: KeyboardEvent) {
    const state = latest.current;
    if (state.open === null) {
      const alone = !event.ctrlKey && !event.shiftKey && !event.metaKey;
      if (event.key !== "Alt" || !alone) {
        return;
      }
      // Alt lands on the first title that opens; with none, the key is left to the window.
      const first = stepTitle(state.menus, -1, 1);
      if (first < 0) {
        return;
      }
      event.preventDefault();
      openMenu(first, firstEnabled(resolve(state.menus[first], state.commands)));
      return;
    }
    const items = resolve(state.menus[state.open], state.commands);
    switch (event.key) {
      case "ArrowDown":
        setCursor(stepOver(items, state.cursor, 1));
        break;
      case "ArrowUp":
        setCursor(stepOver(items, state.cursor, -1));
        break;
      case "ArrowRight":
      case "ArrowLeft": {
        const next = stepTitle(state.menus, state.open, event.key === "ArrowRight" ? 1 : -1);
        if (next >= 0) {
          openMenu(next, firstEnabled(resolve(state.menus[next], state.commands)));
        }
        break;
      }
      case "Enter":
        // The cursor never sits on a greyed item; if the state moved under it, the menu stays open.
        if (state.cursor >= 0 && items[state.cursor].enabled) {
          activate(state.commands, items[state.cursor].id);
        }
        break;
      case "Escape":
        closeMenu(true);
        break;
      default:
        return;
    }
    // The grid moves its own cursor on the same keys, so an open menu keeps them to itself.
    event.preventDefault();
    event.stopPropagation();
  }

  useEffect(() => {
    latest.current = { menus, commands, open, cursor };
  });

  // Registered once: both handlers read `latest`, so a re-render never drops an event.
  useEffect(() => {
    const key = (event: KeyboardEvent) => onKeyDown(event);
    const pointer = (event: MouseEvent) => {
      const inside =
        event.target instanceof Node && barRef.current?.contains(event.target) === true;
      if (!inside && latest.current.open !== null) {
        closeMenu(false);
      }
    };
    window.addEventListener("keydown", key, true);
    window.addEventListener("mousedown", pointer, true);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("mousedown", pointer, true);
    };
  }, []);

  // The keyboard follows the open dropdown, which is what Escape then hands back.
  useLayoutEffect(() => {
    if (open !== null) {
      dropdownRef.current?.focus();
    }
  }, [open]);

  return (
    <div className="menubar" role="menubar" ref={barRef}>
      {menus.map((menu, index) => {
        const items = resolve(menu, commands);
        return (
          <div className="menubar__group" key={menu.id}>
            <button
              className={`menubar__title menubar__title--${menu.id}`}
              type="button"
              role="menuitem"
              aria-haspopup="menu"
              aria-expanded={open === index}
              // Drawn with nothing behind it too, greyed: a title never comes and goes (C2).
              disabled={!opens(menu)}
              onClick={() => (open === index ? closeMenu(false) : openMenu(index, -1))}
            >
              {menu.title}
            </button>
            {open === index && (
              <div
                className="menubar__menu"
                role="menu"
                tabIndex={-1}
                ref={dropdownRef}
                aria-label={menu.title}
                aria-activedescendant={
                  cursor < 0 ? undefined : `menuitem-${commandToken(items[cursor].id)}`
                }
              >
                {items.map((command, position) => (
                  <button
                    className={
                      `menubar__item menubar__item--${commandToken(command.id)}` +
                      (position === cursor ? " menubar__item--cursor" : "")
                    }
                    id={`menuitem-${commandToken(command.id)}`}
                    key={command.id}
                    type="button"
                    // checked+group is one option of a radio set; checked alone is a plain toggle.
                    role={
                      command.checked === undefined
                        ? "menuitem"
                        : command.group === undefined
                          ? "menuitemcheckbox"
                          : "menuitemradio"
                    }
                    aria-checked={command.checked}
                    tabIndex={-1}
                    disabled={!command.enabled}
                    onClick={() => activate(commands, command.id)}
                  >
                    <span className="menubar__item-main">
                      <span className="menubar__mark" aria-hidden="true">
                        {command.checked === true ? (command.group === undefined ? "✓" : "●") : ""}
                      </span>
                      <span className="menubar__label">{command.label}</span>
                    </span>
                    {command.accelerator !== undefined && (
                      <span className="menubar__accelerator">{command.accelerator}</span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
