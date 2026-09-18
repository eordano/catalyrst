import { expect, test } from "vitest";
import { initialEditorPage, editorPageTransition as advance, editorPageDirty } from "./page-machine";

test("saving an earlier revision does not discard edits made during a save", () => {
  const dirty = advance(initialEditorPage(), { type: "edited" });
  const request = { generation: 0, id: 1, command: "save" as const, revision: 1 };
  const saving = advance(dirty, { type: "request", request, allowed: true });
  const edited = advance(saving, { type: "edited" });
  const saved = advance(edited, { type: "finished", request, status: "completed", saved: true });
  expect(saved.savedRevision).toBe(1);
  expect(editorPageDirty(saved)).toBe(true);
});
test("serializes file operations and rejects stale completion after project change", () => {
  const request = { generation: 0, id: 1, command: "publish" as const, revision: 0 };
  const state = advance(initialEditorPage(), { type: "request", request, allowed: true });
  expect(advance(state, { type: "request", request: { ...request, id: 2, command: "open" }, allowed: true })).toBe(state);
  const reset = advance(state, { type: "reset" });
  expect(advance(reset, { type: "finished", request, status: "completed", saved: true })).toBe(reset);
});
test("reconnect invalidates pending results while preserving dirty work", () => {
  const dirty = advance(initialEditorPage(), { type: "edited" });
  const request = { generation: 0, id: 1, command: "save" as const, revision: 1 };
  const pending = advance(dirty, { type: "request", request, allowed: true });
  const reconnected = advance(pending, { type: "connection-changed" });
  expect(editorPageDirty(reconnected)).toBe(true);
  expect(reconnected.pending).toBeNull();
  expect(advance(reconnected, { type: "finished", request, status: "completed", saved: true })).toBe(reconnected);
});
test("cancel and error keep the dirty revision; a cloud copy can count as saved", () => {
  const dirty = advance(initialEditorPage(), { type: "edited" });
  const request = { generation: 0, id: 1, command: "save" as const, revision: 1 };
  const active = advance(dirty, { type: "request", request, allowed: true });
  expect(editorPageDirty(advance(active, { type: "finished", request, status: "cancelled" }))).toBe(true);
  expect(editorPageDirty(advance(active, { type: "finished", request, status: "failed" }))).toBe(true);
  expect(editorPageDirty(advance(active, { type: "finished", request, status: "cancelled", saved: true }))).toBe(false);
});
