// The only place the UI talks to the Rust core.
// Every Tauri command gets a typed wrapper here, so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}
