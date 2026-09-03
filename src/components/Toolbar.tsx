import { Fragment } from "react";

import {
  commandToken,
  runCommand,
  type Command,
  type CommandId,
  type CommandRegistry,
} from "../types/chrome";

type ToolbarProps = {
  /** Groups drawn in order with a divider between them, as ids into `commands` (T3 C1). */
  groups: CommandId[][];
  commands: CommandRegistry;
};

/** A group's ids resolved from the registry's records (T3 C1). */
function resolve(ids: CommandId[], commands: CommandRegistry): Command[] {
  return ids.map((id) => commands[id]);
}

/** The toolbar: ids into the same registry the menu bar draws from, so neither route can grow the other one. */
export default function Toolbar({ groups, commands }: ToolbarProps) {
  return (
    <div className="toolbar">
      {groups.map((ids, index) => {
        const items = resolve(ids, commands);
        return (
          <Fragment key={ids.join("-")}>
            {index > 0 && <span className="toolbar__divider" />}
            {items.map((command) => (
              <button
                className={`toolbar__button toolbar__${commandToken(command.id)}`}
                key={command.id}
                type="button"
                disabled={!command.enabled}
                aria-pressed={command.checked}
                onClick={() => runCommand(commands, command.id)}
              >
                {command.label}
              </button>
            ))}
          </Fragment>
        );
      })}
    </div>
  );
}
