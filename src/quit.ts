import { invoke } from "@tauri-apps/api/core";

/**
 * Quit the app. `AppHandle::exit` is the one quit route, and the backend turns it into the window's
 * own close so it meets the gate that asks about unsaved work. See BACKLOG.md N6.
 */
export async function requestQuit(): Promise<void> {
  await invoke("quit");
}
