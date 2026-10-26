import { openUrl } from '@tauri-apps/plugin-opener';
import { isTauri } from './tauri';

const SAFE_PROTOCOLS = new Set(['http:', 'https:']);

/**
 * Return a normalized URL when it is safe to hand to the operating system,
 * or `null` for anything that is not plain HTTP(S) (javascript:, file:,
 * data:, custom app schemes, malformed input).
 *
 * Model output is untrusted: this check runs on every link before it reaches
 * the opener plugin or `window.open`.
 */
export function safeExternalUrl(raw: string): string | null {
  const value = raw.trim();
  if (!value) return null;
  try {
    const url = new URL(value);
    if (!SAFE_PROTOCOLS.has(url.protocol)) return null;
    return url.toString();
  } catch {
    return null;
  }
}

/**
 * Open an external link in the user's browser. Returns false when the URL was
 * rejected. Failures from the OS opener propagate so the caller can show a
 * friendly error.
 */
export async function openExternal(raw: string): Promise<boolean> {
  const url = safeExternalUrl(raw);
  if (!url) return false;
  if (isTauri) {
    await openUrl(url);
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
  return true;
}
