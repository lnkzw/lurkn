/**
 * Which desktop Lurkn is running on, for the few places that differ: the
 * window controls, and the browser options only Linux can honour.
 *
 * Read from the WebView's user agent rather than asked of Rust: it is known
 * before the first paint, so the title bar never draws the wrong controls for
 * a frame. Each platform's WebView names its OS there — WebKitGTK says Linux,
 * WKWebView says Macintosh, WebView2 says Windows.
 */

export type Platform = 'linux' | 'macos' | 'windows';

export function platformOf(userAgent: string): Platform {
  if (/Macintosh|Mac OS X/.test(userAgent)) return 'macos';
  if (/Windows/.test(userAgent)) return 'windows';
  return 'linux';
}

export const platform: Platform =
  typeof navigator === 'undefined' ? 'linux' : platformOf(navigator.userAgent);
