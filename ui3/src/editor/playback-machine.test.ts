import { expect, test } from "vitest";
import { initialPlayback, playbackTransition as advance, PLAYBACK_TRANSITIONS } from "./playback-machine";

test.each(PLAYBACK_TRANSITIONS)("$command from $from commits only on acknowledgement", ({ from, command, to }) => {
  const state = { ...initialPlayback(7), mode: from, suspended: command === "resume" };
  const request = { generation: 7, id: 1, command };
  const pending = advance(state, { type: "request", request, ready: true });
  expect(pending.mode).toBe(from);
  expect(advance(pending, { type: "completed", request }).mode).toBe(to);
  expect(advance(pending, { type: "failed", request, error: "offline" }).mode).toBe(from);
});
test("blocks unavailable and duplicate commands and stale acknowledgements", () => {
  const state = initialPlayback();
  const request = { generation: 0, id: 1, command: "play" as const };
  expect(advance(state, { type: "request", request, ready: false })).toBe(state);
  const active = advance(state, { type: "request", request, ready: true });
  expect(advance(active, { type: "request", request: { ...request, id: 2 }, ready: true })).toBe(active);
  expect(advance(active, { type: "completed", request: { ...request, id: 2 } })).toBe(active);
  const reset = advance(active, { type: "reset" });
  expect(advance(reset, { type: "completed", request })).toBe(reset);
});
