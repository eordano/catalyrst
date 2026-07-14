import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { act, waitFor } from "@testing-library/react";

import { renderHud } from "../../test/harness";
import { WORLD_CANVAS_ID, WORLD_KEY_CODES, isSynthesizedWorldKey } from "./keys";
import type { WorldKeyCode } from "./keys";
import { getMobileEnv, refreshMobileEnv, setMobileOverride } from "./detect";
import type { SyntheticKeyCode } from "./controls/engineInput";

const ENGINE_CODES: Record<SyntheticKeyCode, string> = {
  KeyW: "w",
  KeyA: "a",
  KeyS: "s",
  KeyD: "d",
  ShiftLeft: "Shift",
  ControlLeft: "Control",
  Space: " ",
  KeyE: "e",
  KeyF: "f",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
};

type SeenKey = { type: string; code: string; defaultPrevented: boolean };

let canvas: HTMLCanvasElement;
let seen: SeenKey[];

function record(e: Event): void {
  const k = e as KeyboardEvent;
  seen.push({ type: k.type, code: k.code, defaultPrevented: k.defaultPrevented });
}

function synthesize(type: "keydown" | "keyup", code: SyntheticKeyCode): void {
  canvas.dispatchEvent(
    new KeyboardEvent(type, {
      code,
      key: ENGINE_CODES[code],
      bubbles: true,
      cancelable: true,
      composed: true,
    }),
  );
}

beforeEach(() => {
  seen = [];
  canvas = document.createElement("canvas");
  canvas.id = WORLD_CANVAS_ID;
  canvas.tabIndex = -1;
  canvas.addEventListener("keydown", record);
  canvas.addEventListener("keyup", record);
  document.body.appendChild(canvas);
});

afterEach(() => {
  canvas.remove();
  setMobileOverride(null);
});

describe("synthesized world key guard", () => {
  test("recognises untrusted world-key codes only", () => {
    expect(isSynthesizedWorldKey(new KeyboardEvent("keydown", { code: "KeyW" }))).toBe(
      true,
    );
    expect(isSynthesizedWorldKey(new KeyboardEvent("keydown", { code: "KeyP" }))).toBe(
      false,
    );

    const trusted = { isTrusted: true, code: "KeyW" } as unknown as KeyboardEvent;
    expect(isSynthesizedWorldKey(trusted)).toBe(false);
  });

  test("covers exactly the codes the touch controls can synthesize", () => {
    const engine = Object.keys(ENGINE_CODES).sort();
    const guarded = [...WORLD_KEY_CODES].sort();
    expect(guarded).toEqual(engine);
  });
});

describe("AppLayout keydown contract", () => {
  test("every synthesized world key reaches the canvas unswallowed", () => {
    const { path } = renderHud();
    seen = [];

    act(() => {
      for (const code of WORLD_KEY_CODES) synthesize("keydown", code);
      for (const code of WORLD_KEY_CODES) synthesize("keyup", code);
    });

    const expected = [
      ...WORLD_KEY_CODES.map((c: WorldKeyCode) => `keydown:${c}`),
      ...WORLD_KEY_CODES.map((c: WorldKeyCode) => `keyup:${c}`),
    ];
    expect(seen.map((s) => `${s.type}:${s.code}`)).toEqual(expected);
    expect(seen.every((s) => !s.defaultPrevented)).toBe(true);
    expect(path()).toBe("/");
  });

  test("movement keys still fall through with a left panel and the emote wheel open", async () => {
    const { user, path } = renderHud();
    await user.keyboard("b");
    seen = [];

    await act(async () => {
      synthesize("keydown", "KeyW");
    });

    expect(seen.map((s) => s.code)).toEqual(["KeyW"]);
    expect(seen.every((s) => !s.defaultPrevented)).toBe(true);
    expect(path()).toBe("/");
  });

  test("non-movement synthesized keys are still intercepted as UI shortcuts", async () => {
    const { path } = renderHud();

    await act(async () => {
      canvas.dispatchEvent(
        new KeyboardEvent("keydown", {
          code: "KeyP",
          key: "p",
          bubbles: true,
          cancelable: true,
        }),
      );
    });

    expect(path()).toBe("/settings");
    expect(seen).toEqual([]);
  });

  test("real keystrokes keep driving the UI shortcuts", async () => {
    const { user, path } = renderHud();
    await user.keyboard("m");
    expect(path()).toBe("/map");
    await user.keyboard("{Escape}");
    expect(path()).toBe("/");
  });
});

describe("desktop chrome suppression", () => {
  test("leaves the document root unstamped on desktop", () => {
    renderHud();
    expect(document.documentElement.hasAttribute("data-mobile-chrome")).toBe(false);
  });

  test("stamps the attribute the desktop-suppression stylesheets key on", () => {
    setMobileOverride("mobile");
    renderHud();
    expect(document.documentElement.hasAttribute("data-mobile-chrome")).toBe(true);
  });
});

describe("mobile touch-control gating", () => {
  async function mountMobileHud() {
    setMobileOverride("mobile");
    const hud = renderHud();
    const controls = await waitFor(() => {
      const el = document.querySelector(".tc");
      if (!el) throw new Error("touch controls did not mount");
      return el;
    });
    return { ...hud, controls };
  }

  test("goes inert while chat is open and comes back when it closes", async () => {
    const { user, controls } = await mountMobileHud();
    expect(controls.getAttribute("data-enabled")).toBe("true");

    await user.keyboard("{Enter}");
    expect(controls.getAttribute("data-enabled")).toBe("false");

    await user.keyboard("{Escape}");
    expect(controls.getAttribute("data-enabled")).toBe("true");
  });

  test("goes inert while the emote wheel is open", async () => {
    const { user, controls } = await mountMobileHud();
    expect(controls.getAttribute("data-enabled")).toBe("true");

    await user.keyboard("b");
    expect(controls.getAttribute("data-enabled")).toBe("false");

    await user.keyboard("b");
    expect(controls.getAttribute("data-enabled")).toBe("true");
  });
});

describe("mobile detection", () => {
  test("defaults to desktop under jsdom and honours the stored override", () => {
    expect(getMobileEnv().isMobile).toBe(false);

    setMobileOverride("mobile");
    expect(getMobileEnv().isMobile).toBe(true);
    expect(getMobileEnv().override).toBe("mobile");

    setMobileOverride("desktop");
    expect(getMobileEnv().isMobile).toBe(false);

    setMobileOverride(null);
    refreshMobileEnv();
    expect(getMobileEnv().override).toBe(null);
  });
});
