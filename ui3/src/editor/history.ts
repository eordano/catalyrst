export interface HistoryEntry {
  entity: string;
  name: string;
  before?: unknown;
  after?: unknown;
}

export type HistoryWrite = (entity: string, name: string, value: unknown) => void;

export interface HistoryEngine {
  push(batch: HistoryEntry[]): void;
  undo(): boolean;
  redo(): boolean;
  canUndo(): boolean;
  canRedo(): boolean;
  isSuppressed(): boolean;
  clear(): void;
}

export const HISTORY_MAX_STEPS = 100;

export function createHistory(
  write: HistoryWrite,
  onChange?: () => void,
  maxSteps: number = HISTORY_MAX_STEPS,
): HistoryEngine {
  const undoStack: HistoryEntry[][] = [];
  const redoStack: HistoryEntry[][] = [];
  let suppress = false;

  const notify = () => {
    try {
      onChange?.();
    } catch {
    }
  };

  const applyBatch = (batch: HistoryEntry[], dir: "before" | "after") => {
    suppress = true;
    try {
      for (const e of batch) {
        write(e.entity, e.name, dir === "before" ? e.before : e.after);
      }
    } finally {
      suppress = false;
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
    undo() {
      const batch = undoStack.pop();
      if (batch === undefined) return false;
      redoStack.push(batch);
      notify();
      applyBatch(batch, "before");
      return true;
    },
    redo() {
      const batch = redoStack.pop();
      if (batch === undefined) return false;
      undoStack.push(batch);
      notify();
      applyBatch(batch, "after");
      return true;
    },
    canUndo: () => undoStack.length > 0,
    canRedo: () => redoStack.length > 0,
    isSuppressed: () => suppress,
    clear() {
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
