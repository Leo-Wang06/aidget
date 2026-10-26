import type { WidgetConfig } from '../types/rust';

export interface WidgetState {
  id: string;
  revision: number;
  token: string;
  /** Native drag region configured in config.json. */
  dragMode: WidgetConfig['dragMode'];
  permissions: WidgetConfig['permissions'];
  hostSurface: boolean;
  surface?: 'host' | 'none' | 'glass';
  title: string;
}

export function sandboxTokens(
  scriptEnabled: boolean,
  dragMode: WidgetState['dragMode'] = 'all',
): string[] {
  // Static documents run the host's injected listener; response CSP blocks their own scripts.
  return scriptEnabled || dragMode === 'all' ? ['allow-scripts'] : [];
}

export function sandboxAttribute(
  scriptEnabled: boolean,
  dragMode: WidgetState['dragMode'] = 'all',
): string {
  return sandboxTokens(scriptEnabled, dragMode).join(' ');
}

/**
 * The part of a page location that decides where widget files are served from.
 *
 * Tauri serves the custom protocol as `aidget://localhost` on macOS and Linux
 * but as `http://aidget.localhost` on Windows, so pages read their own origin
 * instead of assuming one.
 */
export interface PageLocation {
  origin: string;
  hostname: string;
}

/**
 * Origin that serves `/content/…`.
 *
 * A sandboxed widget document is opaque, so only the aidget host page can name
 * the origin; anything else is refused rather than guessed.
 */
export function contentOrigin(location: PageLocation): string {
  if (location.hostname === 'localhost' || location.hostname === 'aidget.localhost') {
    return location.origin;
  }
  throw new Error(`not an aidget page: ${location.hostname}`);
}

export function widgetUrl(origin: string, id: string, revision: number, token: string): string {
  return `${origin}/content/${encodeURIComponent(id)}/index.html?v=${encodeURIComponent(`${revision}-${token}`)}`;
}

/** Keep the child document at its own entrypoint, including after self-navigation. */
export function widgetFramePolicy(origin: string, id: string) {
  return `frame-src ${origin}/content/${encodeURIComponent(id)}/index.html`;
}

/**
 * A Tauri listener without an explicit target is registered as `Any` and
 * receives events emitted to every label. Scope each widget window's listener
 * to its own label so one widget's state never reaches another window.
 */
export function widgetEventTarget(label: string): {
  kind: 'AnyLabel';
  label: string;
} {
  return { kind: 'AnyLabel', label };
}

/** Widget windows are labelled `widget-<id>`; the suffix is the authority. */
export function widgetIdFromLabel(label: string): string | null {
  return label.startsWith('widget-') ? label.slice('widget-'.length) : null;
}

export function isOwnWidget(ownId: string | null, next: WidgetState): boolean {
  return ownId === next.id;
}

export function shouldReload(
  previous: WidgetState | null,
  next: WidgetState,
  currentUrl: string,
  origin: string,
): boolean {
  if (!previous) {
    return true;
  }
  if (previous.revision !== next.revision) {
    return true;
  }
  if (previous.token !== next.token) {
    return true;
  }
  if (previous.permissions.includes('script') !== next.permissions.includes('script')) {
    return true;
  }
  if (previous.dragMode !== next.dragMode) {
    return true;
  }
  if (previous.permissions.join() !== next.permissions.join()) {
    return true;
  }
  return currentUrl !== widgetUrl(origin, next.id, next.revision, next.token);
}
