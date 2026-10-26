import { describe, expect, it, vi } from 'vite-plus/test';

import { afterNextPaint, createMainStartup } from '../../src/main/startup';

describe('main window startup', () => {
  it('waits for two animation frames', async () => {
    const frames: FrameRequestCallback[] = [];
    const painted = afterNextPaint((callback) => frames.push(callback));

    expect(frames).toHaveLength(1);
    frames.shift()!(0);
    expect(frames).toHaveLength(1);
    frames.shift()!(0);
    await painted;
  });

  it('waits for the first paint before starting widgets', async () => {
    let finishPaint: (() => void) | undefined;
    const waitForPaint = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finishPaint = resolve;
        }),
    );
    const startWidgets = vi.fn(async () => true);
    const startup = createMainStartup(startWidgets, vi.fn(), waitForPaint);

    const painted = startup.painted();
    expect(startWidgets).not.toHaveBeenCalled();

    finishPaint!();
    await painted;
    expect(startWidgets).toHaveBeenCalledTimes(1);
  });

  it('starts preview refresh only after the initial widget sync', async () => {
    const refresh = vi.fn();
    const startup = createMainStartup(
      async () => true,
      refresh,
      async () => {},
    );

    await startup.painted();
    expect(refresh).not.toHaveBeenCalled();

    startup.synchronized();
    startup.synchronized();
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('refreshes immediately when widgets were started in the background', async () => {
    const refresh = vi.fn();
    const startup = createMainStartup(
      async () => false,
      refresh,
      async () => {},
    );

    await startup.painted();
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('does not repeat the initial startup when repaint notifications race', async () => {
    const startWidgets = vi.fn(async () => true);
    const refresh = vi.fn();
    let synced = false;
    const startup = createMainStartup(
      async () => {
        synced = true;
        return startWidgets();
      },
      refresh,
      async () => {},
    );

    const first = startup.painted();
    startup.synchronized();
    await first;
    startup.synchronized();

    expect(synced).toBe(true);
    expect(startWidgets).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
