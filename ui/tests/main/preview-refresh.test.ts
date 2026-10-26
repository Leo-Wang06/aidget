import { afterEach, describe, expect, it, vi } from 'vite-plus/test';

import { createPreviewRefresh, PREVIEW_REFRESH_MS } from '../../src/main/preview-refresh';

afterEach(() => {
  vi.useRealTimers();
});

describe('preview refresh scheduler', () => {
  it('runs at the requested cadence', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => {});
    const scheduler = createPreviewRefresh(() => true, refresh);

    scheduler.start();
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS - 1);
    expect(refresh).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).toHaveBeenCalledTimes(2);
    scheduler.stop();
  });

  it('skips ticks while a refresh is still running', async () => {
    vi.useFakeTimers();
    let finish: (() => void) | undefined;
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const scheduler = createPreviewRefresh(() => true, refresh);

    scheduler.start();
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).toHaveBeenCalledTimes(1);

    finish!();
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).toHaveBeenCalledTimes(2);
    scheduler.stop();
  });

  it('does not refresh while the main window is ineligible', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => {});
    let eligible = false;
    const scheduler = createPreviewRefresh(() => eligible, refresh);

    scheduler.start();
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).not.toHaveBeenCalled();

    eligible = true;
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).toHaveBeenCalledTimes(1);
    scheduler.stop();
  });

  it('stops scheduling and supports an immediate guarded refresh', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => {});
    let eligible = true;
    const scheduler = createPreviewRefresh(() => eligible, refresh);

    await scheduler.refreshNow();
    expect(refresh).toHaveBeenCalledTimes(1);

    scheduler.start();
    scheduler.stop();
    await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_MS);
    expect(refresh).toHaveBeenCalledTimes(1);

    eligible = false;
    await scheduler.refreshNow();
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
