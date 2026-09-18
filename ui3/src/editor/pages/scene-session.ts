export type SceneSessionPhase = "handshake" | "hydrating" | "ready" | "error" | "retired";

type SceneSessionEvent = "beginHydration" | "ready" | "fail" | "complete" | "timeout" | "retire";

export interface SceneSessionToken {
  generation: number;
  hydration: number;
}

export const sceneSessionTransitions: Record<
  SceneSessionPhase,
  Partial<Record<SceneSessionEvent, SceneSessionPhase>>
> = {
  handshake: { beginHydration: "hydrating", ready: "ready", timeout: "error", retire: "retired" },
  hydrating: { complete: "ready", fail: "error", timeout: "error", retire: "retired" },
  ready: { retire: "retired" },
  error: { retire: "retired" },
  retired: {},
};

export function reduceSceneSession(phase: SceneSessionPhase, event: SceneSessionEvent) {
  return sceneSessionTransitions[phase][event] ?? phase;
}

export function sceneSessionMermaid() {
  return Object.entries(sceneSessionTransitions)
    .flatMap(([from, events]) => Object.entries(events).map(([event, to]) => `${from} --> ${to}: ${event}`))
    .join("\n");
}

export function createSceneSession(generation: number) {
  let phase: SceneSessionPhase = "handshake";
  let hydration = 0;

  const move = (event: SceneSessionEvent) => {
    const next = reduceSceneSession(phase, event);
    if (next === phase) return false;
    phase = next;
    return true;
  };
  const current = (token: SceneSessionToken) =>
    phase === "hydrating" && token.generation === generation && token.hydration === hydration;

  return {
    generation,
    phase: () => phase,
    isHydrating: current,
    beginHydration: (): SceneSessionToken | null => {
      if (!move("beginHydration")) return null;
      hydration += 1;
      return { generation, hydration };
    },
    readyWithoutHydration: () => move("ready"),
    complete: (token: SceneSessionToken) => current(token) && move("complete"),
    fail: (token: SceneSessionToken) => current(token) && move("fail"),
    timeoutHydration: (token: SceneSessionToken) => current(token) && move("timeout"),
    timeoutHandshake: () => move("timeout"),
    retire: () => {
      move("retire");
    },
  };
}

export function canRestoreSceneSession(
  session: ReturnType<typeof createSceneSession>,
  token: SceneSessionToken,
  busIsCurrent: boolean,
) {
  return busIsCurrent && session.isHydrating(token);
}
