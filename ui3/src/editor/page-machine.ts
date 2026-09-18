export type EditorPageCommand = "save" | "open" | "publish";
export interface EditorPageRequest { generation: number; id: number; command: EditorPageCommand; revision: number }
export interface EditorPageState {
  generation: number;
  revision: number;
  savedRevision: number;
  pending: EditorPageRequest | null;
  outcome: { request: EditorPageRequest; status: "completed" | "cancelled" | "failed"; error?: string } | null;
}
export const initialEditorPage = (generation = 0): EditorPageState => ({ generation, revision: 0, savedRevision: 0, pending: null, outcome: null });
export type EditorPageEvent =
  | { type: "reset" }
  | { type: "edited" }
  | { type: "connection-changed" }
  | { type: "request"; request: EditorPageRequest; allowed: boolean }
  | { type: "finished"; request: EditorPageRequest; status: "completed" | "cancelled" | "failed"; saved?: boolean; error?: string };
export const editorPageTransitions = {
  idle: { request: "busy", edited: "idle", reset: "idle", "connection-changed": "idle" },
  busy: { finished: "idle", edited: "busy", reset: "idle", "connection-changed": "idle" },
} as const;
export const editorPageDiagram = () => "stateDiagram-v2\n" + Object.entries(editorPageTransitions)
  .flatMap(([from, events]) => Object.entries(events).map(([event, to]) => `  ${from} --> ${to}: ${event}`)).join("\n");
export function editorPageTransition(state: EditorPageState, event: EditorPageEvent): EditorPageState {
  if (!(event.type in editorPageTransitions[state.pending ? "busy" : "idle"])) return state;
  if (event.type === "reset") return initialEditorPage(state.generation + 1);
  if (event.type === "connection-changed") return { ...state, generation: state.generation + 1, pending: null,
    outcome: state.pending ? { request: state.pending, status: "failed", error: "The editor reconnected. Check your scene and try again." } : state.outcome };
  if (event.type === "edited") return { ...state, revision: state.revision + 1 };
  if (event.request.generation !== state.generation) return state;
  if (event.type === "request") {
    if (!event.allowed || state.pending || event.request.revision !== state.revision) return state;
    return { ...state, pending: event.request, outcome: null };
  }
  if (state.pending?.id !== event.request.id || state.pending.command !== event.request.command) return state;
  return { ...state, pending: null,
    savedRevision: event.saved ? event.request.revision : state.savedRevision,
    outcome: { request: event.request, status: event.status, error: event.error } };
}
export const editorPageDirty = (state: EditorPageState) => state.revision !== state.savedRevision;
