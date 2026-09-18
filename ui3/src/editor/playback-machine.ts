export type PlaybackMode = "editing" | "playing" | "paused";
export type PlaybackCommand = "play" | "pause" | "step" | "stop" | "suspend" | "resume";
export const PLAYBACK_TRANSITIONS: readonly { from: PlaybackMode; command: PlaybackCommand; to: PlaybackMode }[] = [
  { from: "playing", command: "suspend", to: "playing" },
  { from: "playing", command: "resume", to: "playing" },
  { from: "editing", command: "play", to: "playing" },
  { from: "paused", command: "play", to: "playing" },
  { from: "playing", command: "pause", to: "paused" },
  { from: "paused", command: "step", to: "paused" },
  { from: "playing", command: "stop", to: "editing" },
  { from: "paused", command: "stop", to: "editing" },
];
export interface PlaybackRequest { generation: number; id: number; command: PlaybackCommand }
export interface PlaybackState {
  generation: number;
  mode: PlaybackMode;
  suspended: boolean;
  pending: PlaybackRequest | null;
  outcome: { request: PlaybackRequest; error: string | null } | null;
}
export const initialPlayback = (generation = 0): PlaybackState => ({ generation, mode: "editing", suspended: false, pending: null, outcome: null });
export type PlaybackEvent =
  | { type: "reset" }
  | { type: "request"; request: PlaybackRequest; ready: boolean }
  | { type: "completed"; request: PlaybackRequest }
  | { type: "failed"; request: PlaybackRequest; error: string };
export function canRequestPlayback(state: PlaybackState, command: PlaybackCommand, ready: boolean): boolean {
  return ready && !state.pending &&
    !(command === "suspend" && state.suspended) && !(command === "resume" && !state.suspended) &&
    PLAYBACK_TRANSITIONS.some(row => row.from === state.mode && row.command === command);
}
export function playbackTransition(state: PlaybackState, event: PlaybackEvent): PlaybackState {
  if (event.type === "reset") return initialPlayback(state.generation + 1);
  if (event.request.generation !== state.generation) return state;
  if (event.type === "request") {
    if (!canRequestPlayback(state, event.request.command, event.ready)) return state;
    return { ...state, pending: event.request, outcome: null };
  }
  if (state.pending?.id !== event.request.id || state.pending.command !== event.request.command) return state;
  const next = PLAYBACK_TRANSITIONS.find(row => row.from === state.mode && row.command === event.request.command)!;
  return { ...state, pending: null, suspended: event.type === "failed" ? state.suspended : event.request.command === "suspend" ? true : event.request.command === "pause" || event.request.command === "step" ? state.suspended : false, mode: event.type === "completed" ? next.to : state.mode,
    outcome: { request: event.request, error: event.type === "failed" ? event.error : null } };
}
export const playbackDiagram = () => "stateDiagram-v2\n" + PLAYBACK_TRANSITIONS.map(row => `  ${row.from} --> ${row.to}: ${row.command} acknowledged`).join("\n");
