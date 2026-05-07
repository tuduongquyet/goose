import { revealItemInDir } from "@tauri-apps/plugin-opener";

export async function revealInFileManager(path: string): Promise<void> {
  if (!window.__TAURI_INTERNALS__) return;
  await revealItemInDir(path);
}
