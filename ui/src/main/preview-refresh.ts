export const PREVIEW_REFRESH_MS = 30_000;

/**
 * Run preview refreshes on a fixed cadence without overlapping slow captures.
 * The caller owns eligibility and can request an immediate guarded refresh.
 */
export function createPreviewRefresh(
  canRefresh: () => boolean,
  refresh: () => Promise<void>,
  intervalMs = PREVIEW_REFRESH_MS,
) {
  let timer: ReturnType<typeof setInterval> | undefined;
  let running = false;

  async function run() {
    if (running || !canRefresh()) {
      return;
    }
    running = true;
    try {
      await refresh();
    } finally {
      running = false;
    }
  }

  function start() {
    if (timer !== undefined) {
      return;
    }
    timer = setInterval(() => {
      void run();
    }, intervalMs);
  }

  function stop() {
    if (timer === undefined) {
      return;
    }
    clearInterval(timer);
    timer = undefined;
  }

  return { refreshNow: run, start, stop };
}
