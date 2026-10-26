import type { WidgetState } from './url';

/** One gesture from the current sandboxed widget document. */
export interface DragSignal {
  source: 'aidget-widget';
  type: 'drag';
  phase: 'start' | 'cancel';
  revision: number;
  token: string;
  gesture: number;
}

/** Reject foreign frames, stale navigations, invalid gestures and handle-only widgets. */
export function parseDragSignal(
  event: Pick<MessageEvent, 'data' | 'source'>,
  frame: Window | null,
  state: WidgetState | null,
) {
  if (!frame || event.source !== frame || state?.dragMode !== 'all') {
    return null;
  }
  const data: unknown = event.data;
  if (!data || typeof data !== 'object') {
    return null;
  }
  const signal = data as Partial<DragSignal>;
  if (
    signal.source !== 'aidget-widget' ||
    signal.type !== 'drag' ||
    (signal.phase !== 'start' && signal.phase !== 'cancel') ||
    signal.revision !== state.revision ||
    signal.token !== state.token ||
    typeof signal.gesture !== 'number' ||
    !Number.isSafeInteger(signal.gesture) ||
    signal.gesture <= 0
  ) {
    return null;
  }
  return signal as DragSignal;
}

/** Serialize native drags and honor releases that race the asynchronous begin command. */
export function createDragSession(
  begin: () => Promise<void>,
  start: () => Promise<void>,
  end: () => Promise<void>,
) {
  let current: { cancelled: boolean } | null = null;
  return {
    get active() {
      return current !== null;
    },
    cancel() {
      if (current) {
        current.cancelled = true;
      }
    },
    async start(isCurrent: () => boolean = () => true) {
      if (current) {
        return;
      }
      const gesture = { cancelled: false };
      current = gesture;
      try {
        await begin();
        if (!gesture.cancelled && isCurrent()) {
          await start();
        }
      } finally {
        try {
          await end();
        } finally {
          current = null;
        }
      }
    },
  };
}
