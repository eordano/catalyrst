import { afterEach, expect, it, vi } from "vitest";
import { attachCameraInput } from "./camera-input";

afterEach(() => { document.body.innerHTML = ""; vi.restoreAllMocks(); });

it("leaves character keys, mouse gestures and wheel to the engine during Play", () => {
  const frame = document.createElement("iframe");
  document.body.append(frame);
  const engine = frame.contentWindow!;
  const bus = { setCameraInput: vi.fn(), setCamMode: vi.fn(), focus: vi.fn(), orientAxis: vi.fn(), toggleOrtho: vi.fn() };
  let playing = false;
  const detach = attachCameraInput(engine, bus, () => ({}), () => ({ playing, camMode: "none", activeId: "512" }));
  const editWheel = new WheelEvent("wheel", { deltaY: 100, cancelable: true });
  engine.dispatchEvent(editWheel);
  expect(editWheel.defaultPrevented).toBe(true);
  expect(bus.setCamMode).toHaveBeenCalledWith("target");
  bus.setCamMode.mockClear();
  playing = true;
  const events = [
    new WheelEvent("wheel", { deltaY: 100, cancelable: true }),
    new MouseEvent("pointerdown", { button: 1, cancelable: true }),
    new MouseEvent("dblclick", { cancelable: true }),
    ...["KeyF", "Backquote", "Numpad1", "Numpad5"].map((code) => new KeyboardEvent("keydown", { code, cancelable: true })),
  ];
  for (const event of events) {
    engine.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  }
  expect(bus.setCamMode).not.toHaveBeenCalled();
  expect(bus.focus).not.toHaveBeenCalled();
  expect(bus.orientAxis).not.toHaveBeenCalled();
  expect(bus.toggleOrtho).not.toHaveBeenCalled();
  detach();
});

it("does not replay a queued editor camera movement after entering Play", () => {
  const frame = document.createElement("iframe");
  document.body.append(frame);
  let flush: FrameRequestCallback | undefined;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { flush = cb; return 1; });
  let playing = false;
  const bus = { setCameraInput: vi.fn(), setCamMode: vi.fn() };
  const detach = attachCameraInput(frame.contentWindow!, bus, () => ({}), () => ({ playing, camMode: "target" }));
  frame.contentWindow!.dispatchEvent(new WheelEvent("wheel", { deltaY: 100 }));
  playing = true;
  flush?.(0);
  expect(bus.setCameraInput).not.toHaveBeenCalled();
  playing = false;
  frame.contentWindow!.dispatchEvent(new WheelEvent("wheel", { deltaY: 100 }));
  flush?.(1);
  expect(bus.setCameraInput).toHaveBeenCalledOnce();
  detach();
});

it("does not cancel an orbit when focusing the iframe blurs a host toolbar control", async () => {
  const frame = document.createElement("iframe");
  const button = document.createElement("button");
  document.body.append(button, frame);
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  let flush: FrameRequestCallback | undefined;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { flush = cb; return 1; });
  const bus = { setCameraInput: vi.fn(), setCamMode: vi.fn() };
  const engine = frame.contentWindow!;
  const detach = attachCameraInput(engine, bus, () => ({}), () => ({ camMode: "target" }));
  engine.dispatchEvent(new MouseEvent("pointerdown", { button: 1, clientX: 100, clientY: 100 }));
  button.dispatchEvent(new FocusEvent("blur"));
  window.dispatchEvent(new FocusEvent("blur"));
  await Promise.resolve();
  engine.dispatchEvent(new MouseEvent("pointermove", { clientX: 150, clientY: 120 }));
  flush?.(0);
  expect(bus.setCameraInput).toHaveBeenCalledWith({ orbitYaw: 0.25, orbitPitch: 0.1 });
  detach();
});
