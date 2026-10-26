import { describe, expect, it, vi } from 'vite-plus/test';

import { createDragSession, parseDragSignal } from '../../src/widget/drag-host';
import type { WidgetState } from '../../src/widget/url';

describe('drag message identity', () => {
  const frame = {} as Window;
  const state: WidgetState = {
    id: 'note',
    revision: 2,
    token: 'current',
    dragMode: 'all',
    permissions: [],
    hostSurface: true,
    title: 'Note',
  };
  const data = {
    source: 'aidget-widget',
    type: 'drag',
    phase: 'start',
    revision: 2,
    token: 'current',
    gesture: 1,
  };
  it('accepts only the current document in the owned iframe', () => {
    expect(parseDragSignal({ data, source: frame }, frame, state)).toEqual(data);
    expect(parseDragSignal({ data, source: null }, frame, state)).toBeNull();
    expect(parseDragSignal({ data, source: frame }, frame, { ...state, revision: 3 })).toBeNull();
    expect(
      parseDragSignal({ data, source: frame }, frame, { ...state, token: 'reloaded' }),
    ).toBeNull();
    expect(
      parseDragSignal({ data, source: frame }, frame, { ...state, dragMode: 'handle' }),
    ).toBeNull();
    expect(
      parseDragSignal({ data: { ...data, gesture: -1 }, source: frame }, frame, state),
    ).toBeNull();
    expect(
      parseDragSignal({ data: { ...data, phase: 'move' }, source: frame }, frame, state),
    ).toBeNull();
  });
});

describe('native drag session', () => {
  it('skips duplicates and a release that arrives while begin is pending', async () => {
    let release!: () => void;
    const begin = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const start = vi.fn(async () => {});
    const end = vi.fn(async () => {});
    const session = createDragSession(begin, start, end);
    const pending = session.start();
    expect(session.active).toBe(true);
    await session.start();
    session.cancel();
    release();
    await pending;
    expect(begin).toHaveBeenCalledOnce();
    expect(start).not.toHaveBeenCalled();
    expect(end).toHaveBeenCalledOnce();
    expect(session.active).toBe(false);
  });

  it('rechecks document state before native start and always ends on failure', async () => {
    const start = vi.fn(async () => {
      throw new Error('native failure');
    });
    const end = vi.fn(async () => {});
    const session = createDragSession(async () => {}, start, end);
    await session.start(() => false);
    expect(start).not.toHaveBeenCalled();
    await expect(session.start()).rejects.toThrow('native failure');
    expect(session.active).toBe(false);
    expect(end).toHaveBeenCalledTimes(2);
  });
});
