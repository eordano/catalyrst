import { createSyncEngine, type SyncEngine } from "./sync-engine";

let shared: SyncEngine | null = null;

export function getSharedSyncEngine(): SyncEngine {
  if (!shared) shared = createSyncEngine();
  return shared;
}
