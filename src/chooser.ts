import { invoke } from "@tauri-apps/api/core";

/** The kinds `chooser.rs` knows. An unknown one is refused there before a dialog is raised. */
export type ChooseKind = "project-folder" | "project-file" | "video" | "subtitle" | "subtitle-save";

/**
 * Ask the user for a path through the system chooser. `null` means they cancelled, which every
 * caller treats as "nothing happens" rather than as a failure.
 *
 * `suggested` is a path whose file name a save chooser opens with; the others ignore it.
 */
export async function choosePath(kind: ChooseKind, suggested?: string): Promise<string | null> {
  return await invoke<string | null>("choose_path", { kind, suggested: suggested ?? null });
}
