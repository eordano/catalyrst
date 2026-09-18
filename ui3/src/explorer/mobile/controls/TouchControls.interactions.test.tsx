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
  const jump = view.container.querySelector(".tc__jump");
  if (!stick || !look || !jump) throw new Error("touch controls did not mount");
  measureStick(stick);
  const teardown = () => {
    act(() => view.unmount());
    canvas.remove();
  };
  return { canvas, keys, pointers, stick, look, jump, view, teardown };
}

function downCodes(keys: KeyboardEvent[]) {
  return keys.filter((e) => e.type === "keydown").map((e) => e.code);
}

function upCodes(keys: KeyboardEvent[]) {
  return keys.filter((e) => e.type === "keyup").map((e) => e.code);
}

function pushRight(t: ReturnType<typeof setup>) {
  firePointer(t.stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
  firePointer(t.stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
  expect(downCodes(t.keys)).toEqual(["KeyD"]);
}

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("TouchControls", () => {
  it("holds a movement key while the stick is pushed past the dead zone, ignores a second finger, and releases on lift or pointercancel", () => {
    const { keys, stick } = setup();
    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 122, clientY: 598 });
    expect(downCodes(keys)).toEqual([]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 380 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointerdown", { pointerId: 2, clientX: 60, clientY: 300 });
    firePointer(stick, "pointermove", { pointerId: 2, clientX: 60, clientY: 100 });
    expect(downCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointerup", { pointerId: 1, clientX: 120, clientY: 380 });
    expect(upCodes(keys)).toEqual(["KeyW"]);
    firePointer(stick, "pointerup", { pointerId: 2, clientX: 60, clientY: 100 });

    firePointer(stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(stick, "pointermove", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(downCodes(keys)).toEqual(["KeyW", "KeyD"]);
    firePointer(stick, "pointercancel", { pointerId: 1, clientX: 320, clientY: 600 });
    expect(upCodes(keys)).toEqual(["KeyW", "KeyD"]);
  });

  it("keeps held keys through a resize or orientation change", () => {
    const t = setup();
    pushRight(t);
    act(() => {
      window.dispatchEvent(new Event("resize"));
      window.dispatchEvent(new Event("orientationchange"));
    });
    expect(upCodes(t.keys)).toEqual([]);
    firePointer(t.stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(t.keys)).toEqual(["KeyD", "KeyW"]);
    expect(upCodes(t.keys)).toEqual(["KeyD"]);
  });

  it("releases held keys when the window blurs, the page hides, or the document goes hidden", () => {
    const cases: Array<[string, () => void]> = [
      ["blur", () => window.dispatchEvent(new Event("blur"))],
      ["pagehide", () => window.dispatchEvent(new Event("pagehide"))],
      [
        "visibilitychange",
        () => {
          vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
          document.dispatchEvent(new Event("visibilitychange"));
        },
      ],
    ];
    for (const [name, fire] of cases) {
      const t = setup();
      pushRight(t);
      act(fire);
      expect(upCodes(t.keys), name).toEqual(["KeyD"]);
      t.teardown();
      vi.restoreAllMocks();
    }
  });

  it("drives look and the stick at once from different pointers, and a bare tap on the look surface becomes a left click", () => {
    const drag = setup();
    firePointer(drag.stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(drag.stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    firePointer(drag.look, "pointerdown", { pointerId: 2, clientX: 700, clientY: 300 });
    firePointer(drag.look, "pointermove", { pointerId: 2, clientX: 760, clientY: 300 });
    expect(downCodes(drag.keys)).toEqual(["KeyW"]);
    const move = drag.pointers.find((e) => e.type === "pointermove");
    expect(move?.movementX).toBe(60);
    expect(drag.pointers.some((e) => e.type === "pointerdown" && e.button === 2)).toBe(true);
    drag.teardown();

    const tap = setup();
    firePointer(tap.look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(tap.look, "pointerup", { pointerId: 1, clientX: 702, clientY: 301 });
    const buttons = tap.pointers.filter((e) => e.type === "pointerdown");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]?.button).toBe(0);
  });

  it("cancels look while two fingers are on the look surface, and never clicks once the gesture became a drag", () => {
    const two = setup();
    firePointer(two.look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(two.look, "pointerdown", { pointerId: 2, clientX: 900, clientY: 500 });
    firePointer(two.look, "pointermove", { pointerId: 1, clientX: 800, clientY: 300 });
    expect(two.pointers.filter((e) => e.type === "pointermove")).toHaveLength(0);
    two.teardown();

    const drag = setup();
    firePointer(drag.look, "pointerdown", { pointerId: 1, clientX: 700, clientY: 300 });
    firePointer(drag.look, "pointermove", { pointerId: 1, clientX: 780, clientY: 300 });
    firePointer(drag.look, "pointerup", { pointerId: 1, clientX: 780, clientY: 300 });
    expect(drag.pointers.some((e) => e.type === "pointerdown" && e.button === 0)).toBe(false);
  });

  it("holds Space while the jump button is pressed and releases it when the controls are disabled mid-press", () => {
    const { keys, jump, view } = setup();
    firePointer(jump, "pointerdown", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(downCodes(keys)).toEqual(["Space"]);
    firePointer(jump, "pointerup", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(upCodes(keys)).toEqual(["Space"]);

    firePointer(jump, "pointerdown", { pointerId: 3, clientX: 900, clientY: 700 });
    expect(downCodes(keys)).toEqual(["Space", "Space"]);
    act(() => {
      view.rerender(
        <TouchControls canvasId={CANVAS_ID} restingKnob={false} enabled={false} />,
      );
    });
    expect(upCodes(keys)).toEqual(["Space", "Space"]);
  });

  it("releases everything on unmount and still drives the engine after a StrictMode effect remount", () => {
    const plain = setup();
    pushRight(plain);
    act(() => plain.view.unmount());
    expect(upCodes(plain.keys)).toEqual(["KeyD"]);
    plain.canvas.remove();

    const strict = setup({ strict: true });
    firePointer(strict.stick, "pointerdown", { pointerId: 1, clientX: 120, clientY: 600 });
    firePointer(strict.stick, "pointermove", { pointerId: 1, clientX: 120, clientY: 400 });
    expect(downCodes(strict.keys)).toEqual(["KeyW"]);
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
});
