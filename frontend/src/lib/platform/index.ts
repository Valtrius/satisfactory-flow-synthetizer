import { isTauri } from '@tauri-apps/api/core';
import { createBrowserPlatform } from './browser';
import { createDesktopPlatform } from './desktop';
import type { PlatformServices } from './contracts';

let platform: PlatformServices | undefined;

/** Host selection happens once, shared by jobs, persistence, files and lifecycle. */
export function getPlatform(): PlatformServices {
  return (platform ??= isTauri() ? createDesktopPlatform() : createBrowserPlatform());
}
