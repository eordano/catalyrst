import { StrictMode } from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import TouchControls from "./TouchControls";

const CANVAS_ID = "test-world-canvas";

type PointerBits = {
  pointerId: number;
  clientX: number;
  clientY: number;
  button?: number;
  buttons?: number;
};

function firePointer(el: Element, type: string, bits: PointerBits) {
  act(() => {
    el.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        pointerType: "touch",
        isPrimary: true,
        button: 0,
        buttons: type === "pointerup" || type === "pointercancel" ? 0 : 1,
        ...bits,
      }),
    );
  });
}

function mountCanvas(keys: KeyboardEvent[], pointers: PointerEvent[]) {
  const canvas = document.createElement("canvas");
  canvas.id = CANVAS_ID;
  document.body.appendChild(canvas);
  canvas.addEventListener("keydown", (e) => keys.push(e as KeyboardEvent));
  canvas.addEventListener("keyup", (e) => keys.push(e as KeyboardEvent));
  for (const type of ["pointerdown", "pointerup", "pointermove"]) {
    canvas.addEventListener(type, (e) => pointers.push(e as PointerEvent));
  }
  return canvas;
}

function measureStick(el: Element) {
  vi.spyOn(el, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 400,
    bottom: 800,
    width: 400,
    height: 800,
    toJSON: () => ({}),
  } as DOMRect);
}

function setup(options: { strict?: boolean } = {}) {
  const keys: KeyboardEvent[] = [];
  const pointers: PointerEvent[] = [];
  const canvas = mountCanvas(keys, pointers);
  const controls = <TouchControls canvasId={CANVAS_ID} restingKnob={false} />;
  const view = render(options.strict ? <StrictMode>{controls}</StrictMode> : controls);
  const stick = view.container.querySelector(".tc__stick");
  const look = view.container.querySelector(".tc__look");
  if (!stick || !look) throw new Error("touch controls did not mount");
  measureStick(stick);
  return { canvas, keys, pointers, stick, look, view };
}

function downCodes(keys: KeyboardEvent[]) {
  return keys.filter((e) => e.type === "keydown").map((e) => e.code);
}

function upCodes(keys: KeyboardEvent[]) {
  return keys.filter((e) => e.type === "keyup").map((e) => e.code);
}

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("TouchControls", () => {
  it("holds a movement key while the stick is pushed and releases it on lift", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    expect(downCodes(keys)).toEqual([]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 380 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointerup", { pointerId: 1, clientX: 120, clientY: 380 });
    expect(upCodes(keys)).toEqual(["KeyW"]);
  });

  it("stays still inside the dead zone", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 122, clientY: 598 });
    expect(downCodes(keys)).toEqual([]);
  });

  it("releases held keys on pointercancel", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    firePointer(stick, "pointercancel", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(upCodes(keys)).toEqual(["KeyD"]);
  });

  it("keeps held keys through a resize or orientation change", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    act(() => {
      window.dispatchEvent(new Event("resize"));
      window.dispatchEvent(new Event("orientationchange"));
    });
    expect(upCodes(keys)).toEqual([]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(keys)).toEqual(["KeyD", "KeyW"]);
    expect(upCodes(keys)).toEqual(["KeyD"]);
  });

  it("releases held keys when the window loses focus", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    act(() => {
      window.dispatchEvent(new Event("blur"));
    });
    expect(upCodes(keys)).toEqual(["KeyD"]);
  });

  it("releases held keys when the page is hidden away", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    act(() => {
      window.dispatchEvent(new Event("pagehide"));
    });
    expect(upCodes(keys)).toEqual(["KeyD"]);
  });

  it("releases held keys when the document is hidden", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    const spy = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(upCodes(keys)).toEqual(["KeyD"]);
    spy.mockRestore();
  });

  it("ignores a second finger on the stick", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointerdown", { pointerId: 2, clientX: 60, clientY: 300 });
    firePointer(stick, "pointermove", { pointerId: 2, clientX: 60, clientY: 100 });
    expect(downCodes(keys)).toEqual([]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
  });

  it("drives look and the stick at the same time from different pointers", () => {
    const { keys, pointers, stick, look } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    firePointer(look, "pointerdown", { pointerId: 2, clientX: 700, clientY: 300 });
    firePointer(look, "pointermove", { pointerId: 2, clientX: 760, clientY: 300 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    const move = pointers.find((e) => e.type === "pointermove");
    expect(move?.movementX).toBe(60);
    expect(pointers.some((e) => e.type === "pointerdown" && e.button === 2)).toBe(true);
  });

  it("cancels look while two fingers are on the look surface", () => {
    const { pointers, look } = setup();
    firePointer(look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(look, "pointerdown", { pointerId: 2, clientX: 900, clientY: 500 });
    firePointer(look, "pointermove", { pointerId: 1, clientX: 800, clientY: 300 });
    expect(pointers.filter((e) => e.type === "pointermove")).toHaveLength(0);
  });

  it("synthesises a left click for a tap on the look surface", () => {
    const { pointers, look } = setup();
    firePointer(look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(look, "pointerup", { pointerId: 1, clientX: 702, clientY: 301 });
    const buttons = pointers.filter((e) => e.type === "pointerdown");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]?.button).toBe(0);
  });

  it("does not click when the gesture became a look drag", () => {
    const { pointers, look } = setup();
    firePointer(look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(look, "pointermove", { pointerId: 1, clientX: 780, clientY: 300 });
    firePointer(look, "pointerup", { pointerId: 1, clientX: 780, clientY: 300 });
    expect(pointers.some((e) => e.type === "pointerdown" && e.button === 0)).toBe(false);
  });

  it("holds Space while the jump button is pressed", () => {
    const { keys, view } = setup();
    const jump = view.container.querySelector(".tc__jump");
    if (!jump) throw new Error("jump button did not mount");
    firePointer(jump, "pointerdown", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(downCodes(keys)).toEqual(["Space"]);
    firePointer(jump, "pointerup", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(upCodes(keys)).toEqual(["Space"]);
  });

  it("releases everything on unmount", () => {
    const { keys, stick, view } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyD"]);
    act(() => view.unmount());
    expect(upCodes(keys)).toEqual(["KeyD"]);
  });

  it("still drives the engine after a StrictMode effect remount", () => {
    const { keys, stick } = setup({ strict: true });
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
  });

  it("presses movement once the world canvas appears mid-gesture", () => {
    const keys: KeyboardEvent[] = [];
    const pointers: PointerEvent[] = [];
    const view = render(<TouchControls canvasId={CANVAS_ID} restingKnob={false} />);
    const stick = view.container.querySelector(".tc__stick");
    if (!stick) throw new Error("touch controls did not mount");
    measureStick(stick);
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(keys)).toEqual([]);
    mountCanvas(keys, pointers);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 380 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
  });

  it("releases the jump key when the controls are disabled mid-press", () => {
    const { keys, view } = setup();
    const jump = view.container.querySelector(".tc__jump");
    if (!jump) throw new Error("jump button did not mount");
    firePointer(jump, "pointerdown", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(downCodes(keys)).toEqual(["Space"]);
    act(() => {
      view.rerender(
        <TouchControls canvasId={CANVAS_ID} restingKnob={false} enabled={false} />,
      );
    });
    expect(upCodes(keys)).toEqual(["Space"]);
  });
});
