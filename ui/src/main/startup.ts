export interface MainStartup {
  painted(): Promise<void>;
  synchronized(): void;
}

type ScheduleFrame = (callback: FrameRequestCallback) => void;

/** Resolve after the browser has committed a frame following the current work. */
export function afterNextPaint(schedule: ScheduleFrame = requestAnimationFrame) {
  return new Promise<void>((resolve) => {
    schedule(() => schedule(() => resolve()));
  });
}

/**
 * Coordinate the first main-window paint with one-time widget startup and the
 * first live preview refresh. Background starts can announce synchronization
 * before the painted command resolves, so either event may arrive first.
 */
export function createMainStartup(
  startWidgets: () => Promise<boolean>,
  refreshPreviews: () => void,
  waitForPaint: () => Promise<void> = afterNextPaint,
): MainStartup {
  let painted: Promise<void> | undefined;
  let synchronized = false;
  let initialSyncPending = false;
  let refreshed = false;

  function refreshOnce() {
    if (refreshed) {
      return;
    }
    refreshed = true;
    refreshPreviews();
  }

  async function run() {
    await waitForPaint();
    const startedNow = await startWidgets();
    initialSyncPending = startedNow;
    if (!startedNow || synchronized) {
      refreshOnce();
    }
  }

  return {
    painted() {
      painted ??= run();
      return painted;
    },
    synchronized() {
      synchronized = true;
      if (initialSyncPending) {
        refreshOnce();
      }
    },
  };
}
