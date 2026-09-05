import { useId } from "react";

import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import { type PanelCell, type PublishedPanel } from "../hooks/useModulePanels";
import { commandToken, runCommand, type CommandId, type CommandRegistry } from "../types/chrome";

type ModulePanelProps = {
  panel: PublishedPanel;
  /** The panel's own label, from the item the module contributed. */
  title: string;
  /**
   * The primary row action: the panel's own command.
   *
   * Activating a row activates the panel, which is what module-abi.md 4.1 means by the primary
   * action being the panel's own id. Nothing here knows what that does.
   */
  primary: CommandId;
  /** The items whose parent is this panel, in the order the module described them (4.1). */
  actions: CommandId[];
  commands: CommandRegistry;
  onClose: () => void;
};

/**
 * What a cell draws.
 *
 * The kind picks which of the cell's two values is read and there is no other rule: the core knows
 * how to draw a table and never asks what a row means (module-abi.md 5.3).
 */
function cellText(cell: PanelCell): string {
  switch (cell.kind) {
    case "text":
    case "badge":
      return cell.text;
    case "number":
      return String(cell.number);
    case "percent":
      return fill(en.modules.panel.percent, { value: cell.number });
  }
}

/**
 * A table a module filled, under the grid and inside the panel flow.
 *
 * Headerless on purpose: a `SubloreCell` carries a kind and no column name, so the core has nothing
 * to write in a header and does not invent one.
 *
 * Deliberately not a layer, like the find band beside it. The one place this build draws a module
 * panel never covers the video, so shell-layout.md's rule is met by construction and the `layer`
 * flag a module sets is carried without being read here (module-abi.md 5.4).
 */
export default function ModulePanel({
  panel,
  title,
  primary,
  actions,
  commands,
  onClose,
}: ModulePanelProps) {
  const titleId = useId();
  const primaryCommand: (typeof commands)[CommandId] | undefined = commands[primary];

  return (
    <section
      className={`modulepanel modulepanel--${panel.module}-${panel.panelId}`}
      aria-labelledby={titleId}
    >
      <div className="modulepanel__head">
        <h2 className="modulepanel__title" id={titleId}>
          {title}
        </h2>
        <button className="modulepanel__close" type="button" onClick={onClose}>
          {en.modules.panel.close}
        </button>
      </div>
      <div className="modulepanel__rows" role="table">
        {panel.rows.map((row) => (
          <div className="modulepanel__row" role="row" key={row.handle}>
            <button
              className="modulepanel__activate"
              type="button"
              disabled={primaryCommand === undefined || !primaryCommand.enabled}
              onClick={() => runCommand(commands, primary, row.handle)}
            >
              {row.cells.map((cell, at) => (
                // The position in the row is the whole of what a cell is: it has no id, and the
                // host fixed the row's width before any of this was drawn.
                <span key={at} className={`modulepanel__cell modulepanel__cell--${cell.kind}`}>
                  {cellText(cell)}
                </span>
              ))}
            </button>
            {actions.map((id) => {
              const action = commands[id];
              if (action === undefined) {
                return null;
              }
              // Greyed by the module's own `enable_when`, through the same registry record the
              // menu reads, and run through the same gate.
              return (
                <button
                  className={`modulepanel__action modulepanel__${commandToken(id)}`}
                  key={id}
                  type="button"
                  disabled={!action.enabled}
                  onClick={() => runCommand(commands, id, row.handle)}
                >
                  {action.label}
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </section>
  );
}
