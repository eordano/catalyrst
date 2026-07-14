
export type NodeKind =
  | "route"
  | "state"
  | "modal"
  | "outcome"
  | "chip"
  | "external"
  | "jump"
  | "end"
  | "sep";

export type EdgeKind =
  | "click"
  | "load"
  | "auto"
  | "reversible"
  | "step";

export interface FlowNode {
  t: "node";
  kind: NodeKind;
  label: string;
  sub?: string;
  href?: string;
  plain?: boolean;
  title?: string;
  busy?: boolean;
  chains?: string[];
}

export interface FlowEdge {
  t: "edge";
  kind: EdgeKind;
  label?: string;
  work?: string;
  chains?: string[];
}

export type TrackItem = FlowNode | FlowEdge;

export interface Track {
  chain?: string;
  chips?: boolean;
  note?: string;
  items: TrackItem[];
  branches?: Track[];
}

export interface FlowSection {
  id: string;
  num: string;
  title: string;
  machines?: string[];
  blurb: string;
  tracks: Track[];
}

export interface FlowStats {
  routes: number;
  states: number;
  clicks: number;
  loads: number;
}


export const node = (kind: NodeKind, label: string, o: Partial<FlowNode> = {}): FlowNode => ({
  t: "node",
  kind,
  label,
  ...o,
});
export const route = (label: string, o: Partial<FlowNode> = {}) =>
  node("route", label, { href: label, ...o });
export const state = (label: string, o: Partial<FlowNode> = {}) => node("state", label, o);
export const outcome = (label: string, o: Partial<FlowNode> = {}) => node("outcome", label, o);
export const sep = (): FlowNode => node("sep", "·");

export const click = (label: string, o: Partial<FlowEdge> = {}): FlowEdge => ({
  t: "edge",
  kind: "click",
  label,
  ...o,
});
export const load = (work?: string, o: Partial<FlowEdge> = {}): FlowEdge => ({
  t: "edge",
  kind: "load",
  work,
  ...o,
});
export const auto = (label: string, o: Partial<FlowEdge> = {}): FlowEdge => ({
  t: "edge",
  kind: "auto",
  label,
  ...o,
});
export const step = (): FlowEdge => ({ t: "edge", kind: "step" });


export function computeStats(sections: FlowSection[]): FlowStats {
  const routes = new Set<string>();
  const states = new Set<string>();
  let clicks = 0;
  let loads = 0;

  const walk = (t: Track) => {
    for (const it of t.items) {
      if (it.t === "node") {
        if (it.kind === "route") routes.add(it.label);
        if (it.kind === "state" || it.kind === "modal") states.add(it.label);
        if (it.busy) loads += 1;
      } else {
        if (it.kind === "load") {
          loads += 1;
          if (it.label) clicks += 1;
        } else if (it.kind === "click" || it.kind === "reversible" || it.kind === "step") {
          clicks += 1;
        }
      }
    }
    t.branches?.forEach(walk);
  };
  sections.forEach((s) => s.tracks.forEach(walk));

  return { routes: routes.size, states: states.size, clicks, loads };
}
