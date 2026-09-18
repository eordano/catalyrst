import type { BootState } from "./boot-machine";
import { canRequestPlayback, initialPlayback, type PlaybackState } from "./playback-machine";

export interface EditorWorkspaceSnapshot {
  session: number;
  sequence: number;
  destination: string | null;
  phase: "preparing" | "connecting" | "editing" | "playing" | "paused" | "error";
  blockingReason: string | null;
  readiness: { project: boolean; engine: boolean; scene: boolean };
  progress: { stage: BootState["stage"]; percent: number | null };
  capabilities: { edit: boolean; save: boolean; open: boolean; publish: boolean; play: boolean; pause: boolean; stop: boolean; step: boolean; debug: boolean };
  request: PlaybackState["pending"];
  outcome: PlaybackState["outcome"];
  ready: boolean;
  playing: boolean;
  busy: boolean;
}

export function workspaceSnapshot(facts: {
  session: number;
  sequence: number;
  destination: string | null;
  project: "pending" | "ready" | "error";
  engine: { ready: boolean; error: string | null; stage: BootState["stage"]; progress: number | null };
  scene: { ready: boolean; error: string | null };
  playback: PlaybackState;
  operationsBusy: boolean;
}): EditorWorkspaceSnapshot {
  const { playback } = facts;
  const error = facts.project === "error" ? "The project files could not be prepared." : facts.scene.error || facts.engine.error;
  const readiness = { project: facts.project === "ready", engine: facts.engine.ready, scene: facts.scene.ready };
  const ready = !error && readiness.project && readiness.engine && readiness.scene;
  const playing = playback.mode !== "editing";
  const available = ready && !facts.operationsBusy;
  const idle = !facts.operationsBusy && !playback.pending;
  const edit = available && idle && !playing;
  const blockingReason = error || (!readiness.project ? "Preparing project files." : !readiness.engine ? "Starting the engine." : !readiness.scene ? "Waiting for the scene to finish loading." : playback.pending ? `Waiting for ${playback.pending.command} to complete.` : facts.operationsBusy ? "A project operation is in progress." : playing ? "Stop the preview before editing." : null);
  return {
    session: facts.session, sequence: facts.sequence, destination: facts.destination,
    phase: error ? "error" : !readiness.project ? "preparing" : !ready ? "connecting" : playback.mode,
    blockingReason, readiness, progress: { stage: facts.engine.stage, percent: facts.engine.progress },
    capabilities: {
      edit, save: edit, publish: edit, open: idle && !playing,
      play: canRequestPlayback(playback, "play", available),
      pause: canRequestPlayback(playback, "pause", available),
      stop: canRequestPlayback(playback, "stop", available),
      step: canRequestPlayback(playback, "step", available),
      debug: available && idle && playing,
    },
    request: playback.pending, outcome: playback.outcome, ready, playing, busy: !!playback.pending,
  };
}

export const INITIAL_WORKSPACE = workspaceSnapshot({ session: 0, sequence: 0, destination: null, project: "pending",
  engine: { ready: false, error: null, stage: null, progress: null }, scene: { ready: false, error: null }, playback: initialPlayback(), operationsBusy: false });
