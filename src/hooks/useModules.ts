import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Why a module file was not used, as the backend reports it. Sentences are assembled here. */
export type ModuleRefusal =
  | { kind: "unopenable"; detail: string }
  | { kind: "notAModule" }
  | { kind: "versionDiffers"; ours: number; theirs: number }
  | { kind: "revisionTooNew"; ours: number; theirs: number }
  | { kind: "tableSize"; ours: number; theirs: number }
  | { kind: "refused"; code: number };

export type ModuleRefused = { file: string } & ModuleRefusal;

export type ModuleReport = {
  loaded: string[];
  refused: ModuleRefused[];
  /** The launch asked for no modules at all, with `--no-modules`. */
  skipped: boolean;
};

const NOTHING: ModuleReport = { loaded: [], refused: [], skipped: false };

/**
 * What the startup scan found, asked for once.
 *
 * The scan already ran before the window existed, so this reads a result rather than starting one.
 * A failure to read it is treated as nothing found: the report only ever adds a line to the screen,
 * and an app that could not ask about modules is an app with no modules as far as the user is
 * concerned. See docs/module-abi.md 3.5.
 */
export function useModules(): ModuleReport {
  const [report, setReport] = useState<ModuleReport>(NOTHING);

  useEffect(() => {
    let listening = true;
    void invoke<ModuleReport>("module_report").then(
      (found) => {
        if (listening) {
          setReport(found);
        }
      },
      (error) => {
        console.error("module report: the scan's result could not be read", error);
      },
    );
    return () => {
      listening = false;
    };
  }, []);

  return report;
}

/**
 * The one sentence a refused module gets, assembled from a core string and the numbers.
 *
 * The file name comes from the directory and the versions are integers, so nothing here is a word
 * the core had to know: that is what keeps the open repository free of the product's own
 * vocabulary while still saying something a user can act on.
 */
export function refusalLine(refused: ModuleRefused, strings: ModuleStrings): string {
  const reason = (() => {
    switch (refused.kind) {
      case "notAModule":
        return strings.notAModule;
      case "versionDiffers":
        return fillPair(strings.versionDiffers, refused.theirs, refused.ours);
      case "revisionTooNew":
        return fillPair(strings.revisionTooNew, refused.theirs, refused.ours);
      case "tableSize":
        return fillPair(strings.tableSize, refused.theirs, refused.ours);
      case "refused":
        return strings.refused.replace("{code}", String(refused.code));
      case "unopenable":
        return strings.unopenable;
    }
  })();
  return strings.line.replace("{file}", refused.file).replace("{reason}", reason);
}

/** The strings `refusalLine` needs, so this file holds no copy of them. */
export type ModuleStrings = {
  line: string;
  notAModule: string;
  versionDiffers: string;
  revisionTooNew: string;
  tableSize: string;
  refused: string;
  unopenable: string;
};

function fillPair(template: string, theirs: number, ours: number): string {
  return template.replace("{theirs}", String(theirs)).replace("{ours}", String(ours));
}
