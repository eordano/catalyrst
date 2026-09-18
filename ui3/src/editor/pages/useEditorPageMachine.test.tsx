import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { useEditorPageMachine } from "./useEditorPageMachine";
import { editorPageDirty, type EditorPageRequest } from "../page-machine";

afterEach(cleanup);

test("rejects duplicate commands synchronously, before React renders", () => {
  const hook = renderHook(() => useEditorPageMachine("project-a"));
  let request: EditorPageRequest | null = null;
  act(() => {
    request = hook.result.current.begin("publish", true);
    expect(hook.result.current.begin("publish", true)).toBeNull();
    expect(hook.result.current.begin("save", true)).toBeNull();
  });
  expect(request).not.toBeNull();
  expect(hook.result.current.state.pending).toBe(request);
});

test("invalidates async work on unmount and project replacement", () => {
  const hook = renderHook(({ scope }) => useEditorPageMachine(scope), { initialProps: { scope: "project-a" } });
  const first = hook.result.current;
  let old!: EditorPageRequest;
  act(() => { old = first.begin("save", true)!; });
  hook.rerender({ scope: "project-b" });
  expect(first.accepts(old)).toBe(false);
  let next!: EditorPageRequest;
  act(() => { next = hook.result.current.begin("save", true)!; });
  expect(next.generation).not.toBe(old.generation);
  const second = hook.result.current;
  hook.unmount();
  expect(second.accepts(next)).toBe(false);
});

test("a saved snapshot cannot clear a more recent edit, and reconnect preserves it", () => {
  const hook = renderHook(() => useEditorPageMachine("project-a"));
  let save!: EditorPageRequest;
  act(() => {
    hook.result.current.send({ type: "edited" });
    save = hook.result.current.begin("save", true)!;
    hook.result.current.send({ type: "edited" });
    hook.result.current.send({ type: "finished", request: save, status: "completed", saved: true });
  });
  expect(editorPageDirty(hook.result.current.state)).toBe(true);
  act(() => hook.result.current.send({ type: "connection-changed" }));
  expect(editorPageDirty(hook.result.current.state)).toBe(true);
});

test("abort signals retire with their operation and do not cancel a replacement request", () => {
  const hook = renderHook(() => useEditorPageMachine("project-a"));
  let first!: EditorPageRequest;
  act(() => { first = hook.result.current.begin("save", true)!; });
  const oldSignal = hook.result.current.signal(first);
  expect(oldSignal.aborted).toBe(false);
  act(() => hook.result.current.send({ type: "connection-changed" }));
  expect(oldSignal.aborted).toBe(true);
  let next!: EditorPageRequest;
  act(() => { next = hook.result.current.begin("save", true)!; });
  expect(hook.result.current.signal(next).aborted).toBe(false);
});
