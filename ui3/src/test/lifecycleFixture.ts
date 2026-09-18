import type { LifecycleSnapshot } from "../generated/bridge/LifecycleSnapshot";

export function lifecycleFixture(overrides: Partial<LifecycleSnapshot> = {}): LifecycleSnapshot {
  return {
    session: "engine-a", revision: 1,
    realm: { phase: "active", generation: 1, destination: "https://realm.invalid", error: null },
    travel: null,
    readiness: { placement: "placed", scene: "running", avatarReady: true, movementReady: true, pendingAssets: 0, canExplore: true, globalRoom: null, sceneRoom: null },
    outcomes: [], commandResults: [], connections: [], sceneRoom: null, ...overrides,
  };
}
