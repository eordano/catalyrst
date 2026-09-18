export type ProjectRealmStatus = "pending" | "ready" | "error";
export type ProjectRealmEvent =
  | { type: "start"; request: number; session: number }
  | { type: "ready"; request: number; session: number }
  | { type: "error"; request: number; session: number };

export interface ProjectRealmMachine {
  status: ProjectRealmStatus;
  request: number;
  session: number;
}

export const initialProjectRealmMachine = (project: boolean): ProjectRealmMachine => ({
  status: project ? "pending" : "ready", request: 0, session: 0,
});

/** Pure transition: effects must carry the generation they started with. */
export const projectRealmTransitions: Record<ProjectRealmStatus, Partial<Record<ProjectRealmEvent["type"], ProjectRealmStatus>>> = {
  pending: { start: "pending", ready: "ready", error: "error" },
  ready: { start: "pending" },
  error: { start: "pending" },
};
export const projectRealmDiagram = () => "stateDiagram-v2\n" + Object.entries(projectRealmTransitions)
  .flatMap(([from, events]) => Object.entries(events).map(([event, to]) => `  ${from} --> ${to}: ${event}`)).join("\n");
export function transitionProjectRealm(state: ProjectRealmMachine, event: ProjectRealmEvent): ProjectRealmMachine {
  const status = projectRealmTransitions[state.status][event.type];
  if (!status) return state;
  if (event.type === "start") return { status, request: event.request, session: event.session };
  if (event.request !== state.request || event.session !== state.session || state.status !== "pending") return state;
  return { ...state, status };
}
