import { writeText } from '@tauri-apps/plugin-clipboard-manager';

/**
 * Copy text, preferring the Tauri plugin and falling back to the WebView API.
 *
 * The fallback matters in `vite dev` opened in a plain browser, where the Tauri
 * bridge is absent. Returns false rather than throwing so callers can show a
 * "select it manually" hint instead of an error.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await writeText(text);
    return true;
  } catch (error) {
    console.warn('clipboard plugin unavailable, trying the WebView API', error);
  }

  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (error) {
    console.warn('could not copy to the clipboard', error);
    return false;
  }
}
