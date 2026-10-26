import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './tauri';

/**
 * Subscribe to a core event. Returns a no-op unsubscribe outside Tauri so the
 * UI can still be previewed in a browser.
 */
export async function listenEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri) return () => {};
  return listen<T>(name, (event) => handler(event.payload));
}
