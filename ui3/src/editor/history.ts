export interface HistoryEntry {
  entity: string;
  name: string;
  before?: unknown;
  after?: unknown;
}

type HistoryWrite = (entity: string, name: string, value: unknown) => unknown;

export interface HistoryEngine {
  push(batch: HistoryEntry[]): void;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  canUndo(): boolean;
  canRedo(): boolean;
  isSuppressed(): boolean;
  clear(): void;
}

const HISTORY_MAX_STEPS = 100;

export function createHistory(
  write: HistoryWrite,
  onChange?: () => void,
  maxSteps: number = HISTORY_MAX_STEPS,
): HistoryEngine {
  const undoStack: HistoryEntry[][] = [];
  const redoStack: HistoryEntry[][] = [];
  let suppress = false;
  let generation = 0;

  const notify = () => {
    try {
      onChange?.();
    } catch {
    }
  };

  const replay = async (from: HistoryEntry[][], to: HistoryEntry[][], dir: "before" | "after") => {
    if (suppress || !from.length) return false;
    const batch = from[from.length - 1]!;
    const version = generation;
    const completed: HistoryEntry[] = [];
    suppress = true;
    notify();
    try {
      for (const entry of batch) {
        if (version !== generation) return false;
        await write(entry.entity, entry.name, entry[dir]);
        completed.push(entry);
      }
      if (version !== generation) return false;
      from.pop();
      to.push(batch);
      return true;
    } catch (error) {
      if (version === generation) {
        const failures: unknown[] = [];
        for (const entry of completed.reverse()) {
          try { await write(entry.entity, entry.name, entry[dir === "before" ? "after" : "before"]); }
          catch (rollbackError) { failures.push(rollbackError); }
        }
        if (failures.length) throw new Error(`History could not be fully restored. Reconnect and check the scene. ${String(error)}`);
      }
      throw error;
    } finally {
      suppress = false;
      notify();
    }
  };

  return {
    push(batch) {
      if (suppress || !Array.isArray(batch) || batch.length === 0) return;
      undoStack.push(batch);
      if (undoStack.length > maxSteps) undoStack.shift();
      redoStack.length = 0;
      notify();
    },
    undo: () => replay(undoStack, redoStack, "before"),
    redo: () => replay(redoStack, undoStack, "after"),
    canUndo: () => !suppress && undoStack.length > 0,
    canRedo: () => !suppress && redoStack.length > 0,
    isSuppressed: () => suppress,
    clear() {
      generation++;
      undoStack.length = 0;
      redoStack.length = 0;
      notify();
    },
  };
}

export function cloneValue<T>(v: T): T {
  if (v === undefined || v === null || typeof v !== "object") return v;
  try {
    return JSON.parse(JSON.stringify(v)) as T;
  } catch {
    return v;
  }
}
