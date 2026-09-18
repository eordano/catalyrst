import { expect, test } from "vitest";
import { initialPlayback, playbackTransition } from "./playback-machine";
import { workspaceSnapshot } from "./workspace-lifecycle";

const facts = { session: 1, sequence: 7, destination: "/play?editorUi=1", project: "ready" as const,
  engine: { ready: true, error: null, stage: null, progress: 100 }, scene: { ready: true, error: null }, playback: initialPlayback(), operationsBusy: false };

test("engine readiness does not imply scene hydration or editable state", () => {
  const snapshot = workspaceSnapshot({ ...facts, scene: { ready: false, error: null } });
  expect(snapshot.readiness).toEqual({ project: true, engine: true, scene: false });
  expect(snapshot.ready).toBe(false);
  expect(snapshot.capabilities).toMatchObject({ save: false, publish: false, play: false, open: true });
  expect(snapshot.blockingReason).toContain("scene");
});
test("capabilities track the same acknowledged playback transition table", () => {
  const request = { generation: 0, id: 1, command: "play" as const };
  const pending = playbackTransition(facts.playback, { type: "request", request, ready: true });
  const snapshot = workspaceSnapshot({ ...facts, playback: pending });
  expect(snapshot.capabilities).toMatchObject({ save: false, play: false, pause: false, stop: false });
  expect(snapshot.request).toBe(request);
  const running = workspaceSnapshot({ ...facts, playback: playbackTransition(pending, { type: "completed", request }) });
  expect(running.capabilities).toMatchObject({ save: false, pause: true, stop: true });
});
test("offline editor can open a replacement project and exposes its blocking error", () => {
  const snapshot = workspaceSnapshot({ ...facts, engine: { ...facts.engine, ready: false, error: "Worker stopped" } });
  expect(snapshot.phase).toBe("error");
  expect(snapshot.blockingReason).toBe("Worker stopped");
  expect(snapshot.capabilities.open).toBe(true);
  expect(snapshot.capabilities.edit).toBe(false);
});
