// Simple typed event bus — replaces Tauri's listen() / emit().

type Callback<T = unknown> = (event: { payload: T }) => void;

const listeners = new Map<string, Set<Callback>>();

/**
 * Subscribe to an event. Returns an unlisten function.
 * API mirrors `@tauri-apps/api/event#listen`.
 */
export function listen<T = unknown>(
  eventName: string,
  callback: Callback<T>,
): Promise<() => void> {
  if (!listeners.has(eventName)) listeners.set(eventName, new Set());
  const set = listeners.get(eventName)!;
  set.add(callback as Callback);
  return Promise.resolve(() => set.delete(callback as Callback));
}

/** Emit an event to all subscribers. */
export function emit<T = unknown>(eventName: string, payload: T): void {
  for (const cb of listeners.get(eventName) ?? []) {
    try {
      cb({ payload });
    } catch (err) {
      console.error(`[event-bus] listener error for "${eventName}":`, err);
    }
  }
}
