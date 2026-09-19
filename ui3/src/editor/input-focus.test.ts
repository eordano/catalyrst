import { afterEach, expect, it, vi } from "vitest";
import { forwardEngineKeys } from "./shortcuts";

afterEach(() => vi.restoreAllMocks());

it("forwards modifier shortcuts to body/document/window, refocuses the viewport, and detaches", () => {
  const engine = new EventTarget() as unknown as Window;
  const focus = vi.fn();
  const key = vi.fn();
  document.body.addEventListener("keydown", key);
  const detach = forwardEngineKeys(engine, { iframe: { focus } as unknown as HTMLIFrameElement });
  const event = new KeyboardEvent("keydown", { key: "s", ctrlKey: true, keyCode: 83, cancelable: true });
  engine.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(key).toHaveBeenCalledOnce();
  expect(key.mock.calls[0]![0].keyCode).toBe(83);
  engine.dispatchEvent(new Event("pointerdown"));
  expect(focus).toHaveBeenCalledOnce();
  detach();
  engine.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
  expect(key).toHaveBeenCalledOnce();
  document.body.removeEventListener("keydown", key);
});

it("keeps scene input local during Play while retaining modified editor shortcuts", () => {
  const engine = new EventTarget() as unknown as Window;
  const key = vi.fn();
  window.addEventListener("keydown", key);
  const detach = forwardEngineKeys(engine, { isEditingEnabled: () => false });
  for (const value of ["w", "Delete", "r"]) engine.dispatchEvent(new KeyboardEvent("keydown", { key: value }));
  expect(key).not.toHaveBeenCalled();
  engine.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true }));
  expect(key).toHaveBeenCalledOnce();
  detach();
  window.removeEventListener("keydown", key);
});
