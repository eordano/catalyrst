import { describe, expect, it } from "vitest";
import {
  FORWARDED_KEYS,
  isTypingTarget,
  shortcutActionFor,
  shortcutGroups,
  type EditorShortcutContext,
} from "./shortcuts";

const ctx = (over: Partial<EditorShortcutContext> = {}): EditorShortcutContext => ({
  playing: false,
  camMode: "target",
  overlayOpen: false,
  menuOrModalOpen: false,
  ...over,
});

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("tool letters (Q/W/E/R)", () => {
  it.each([
    ["q", "select"],
    ["w", "translate"],
    ["e", "rotate"],
    ["r", "scale"],
  ])("%s \u{2192} %s in the static edit camera", (k, tool) => {
    expect(shortcutActionFor(key({ key: k }), ctx())).toEqual({ type: "tool", tool });
    expect(shortcutActionFor(key({ key: k }), ctx({ camMode: "none" }))).toEqual({
      type: "tool",
      tool,
    });
  });

  it("never fire while the scene is playing (letters mean movement)", () => {
    expect(shortcutActionFor(key({ key: "w" }), ctx({ playing: true }))).toBeNull();
    expect(shortcutActionFor(key({ key: "q" }), ctx({ playing: true }))).toBeNull();
  });

  it("never fire in the fly camera (WASD flies)", () => {
    expect(shortcutActionFor(key({ key: "w" }), ctx({ camMode: "free" }))).toBeNull();
    expect(shortcutActionFor(key({ key: "e" }), ctx({ camMode: "free" }))).toBeNull();
  });

  it("ignore modifier combos (\u{2318}W must stay close-tab, Alt+E is not a tool)", () => {
    expect(shortcutActionFor(key({ key: "w", metaKey: true }), ctx())).toBeNull();
    expect(shortcutActionFor(key({ key: "e", altKey: true }), ctx())).toBeNull();
  });
});

describe("edit keys", () => {
  it("\u{2318}Z / Ctrl+Z \u{2192} undo, with \u{21E7} \u{2192} redo \u{2014} in every camera state, even playing", () => {
    for (const c of [ctx(), ctx({ camMode: "free" }), ctx({ playing: true })]) {
      expect(shortcutActionFor(key({ key: "z", metaKey: true }), c)).toEqual({ type: "undo" });
      expect(shortcutActionFor(key({ key: "z", ctrlKey: true }), c)).toEqual({ type: "undo" });
      expect(shortcutActionFor(key({ key: "z", metaKey: true, shiftKey: true }), c)).toEqual({
        type: "redo",
      });
    }
  });

  it("\u{2318}D / Ctrl+D \u{2192} duplicate", () => {
    expect(shortcutActionFor(key({ key: "d", metaKey: true }), ctx())).toEqual({
      type: "duplicate",
    });
    expect(shortcutActionFor(key({ key: "d", ctrlKey: true }), ctx())).toEqual({
      type: "duplicate",
    });
    expect(shortcutActionFor(key({ key: "d" }), ctx())).toBeNull();
  });

  it("Delete / Backspace \u{2192} delete selection; modified variants pass through", () => {
    expect(shortcutActionFor(key({ key: "Delete" }), ctx())).toEqual({ type: "delete" });
    expect(shortcutActionFor(key({ key: "Backspace" }), ctx())).toEqual({ type: "delete" });
    expect(shortcutActionFor(key({ key: "Delete", ctrlKey: true }), ctx())).toBeNull();
  });
});

describe("control keys", () => {
  it("F5 \u{2192} play", () => {
    expect(shortcutActionFor(key({ key: "F5" }), ctx())).toEqual({ type: "play" });
  });

  it("? toggles the overlay; Esc clears the selection", () => {
    expect(shortcutActionFor(key({ key: "?", shiftKey: true }), ctx())).toEqual({
      type: "toggle-overlay",
    });
    expect(shortcutActionFor(key({ key: "Escape" }), ctx())).toEqual({
      type: "clear-selection",
    });
  });

  it("while the overlay is open, ? and Esc close it and everything else stands down", () => {
    const open = ctx({ overlayOpen: true });
    expect(shortcutActionFor(key({ key: "?" }), open)).toEqual({ type: "close-overlay" });
    expect(shortcutActionFor(key({ key: "Escape" }), open)).toEqual({ type: "close-overlay" });
    expect(shortcutActionFor(key({ key: "w" }), open)).toBeNull();
    expect(shortcutActionFor(key({ key: "Delete" }), open)).toBeNull();
  });

  it("menus/modals take priority: nothing fires, Esc is left to their own handlers", () => {
    const modal = ctx({ menuOrModalOpen: true });
    for (const k of ["Escape", "w", "Delete", "F5", "?"]) {
      expect(shortcutActionFor(key({ key: k }), modal)).toBeNull();
    }
    expect(shortcutActionFor(key({ key: "z", metaKey: true }), modal)).toBeNull();
  });

  it("'.' steps one tick ONLY while the debug panel is open", () => {
    expect(
      shortcutActionFor(key({ key: "." }), ctx({ playing: true, debugOpen: true })),
    ).toEqual({ type: "step-tick" });
    expect(shortcutActionFor(key({ key: "." }), ctx())).toBeNull();
    expect(shortcutActionFor(key({ key: "." }), ctx({ playing: true }))).toBeNull();
    expect(
      shortcutActionFor(key({ key: ".", metaKey: true }), ctx({ debugOpen: true })),
    ).toBeNull();
    expect(
      shortcutActionFor(key({ key: "." }), ctx({ debugOpen: true, menuOrModalOpen: true })),
    ).toBeNull();
  });
});

describe("typing guard", () => {
  it.each(["input", "textarea", "select"])("suppresses shortcuts typed into a <%s>", (tag) => {
    const el = document.createElement(tag);
    document.body.appendChild(el);
    let seen: unknown = "unset";
    const onKey = (e: Event) => {
      seen = shortcutActionFor(e as KeyboardEvent, ctx());
    };
    window.addEventListener("keydown", onKey, { capture: true });
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "w", bubbles: true }));
    window.removeEventListener("keydown", onKey, { capture: true });
    el.remove();
    expect(seen).toBeNull();
  });

  it("suppresses shortcuts in contentEditable surfaces", () => {
    expect(
      isTypingTarget({
        composedPath: () => [{ tagName: "DIV", isContentEditable: true } as unknown as EventTarget],
        target: null,
      }),
    ).toBe(true);
  });

  it("does not treat plain buttons or the window as typing", () => {
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    let seen: unknown = "unset";
    const onKey = (e: Event) => {
      seen = shortcutActionFor(e as KeyboardEvent, ctx());
    };
    window.addEventListener("keydown", onKey, { capture: true });
    btn.dispatchEvent(new KeyboardEvent("keydown", { key: "q", bubbles: true }));
    window.removeEventListener("keydown", onKey, { capture: true });
    btn.remove();
    expect(seen).toEqual({ type: "tool", tool: "select" });
    expect(shortcutActionFor(key({ key: "q" }), ctx())).toEqual({ type: "tool", tool: "select" });
  });
});

describe("cheatsheet completeness", () => {
  it("documents every discrete binding plus the mouse scheme, views, backtick and F", () => {
    const combos = shortcutGroups({ preset: "blender", mac: true })
      .flatMap((g) => g.items.map((i) => i.combo))
      .join(" | ");
    for (const expected of [
      "Q",
      "W",
      "E",
      "R",
      "\u{2318} Z",
      "\u{2318} \u{21E7} Z",
      "\u{2318} D",
      "Del",
      "F5",
      "Esc",
      "?",
      "F",
      "`",
      "MMB",
      "Scroll",
      "Numpad 1 / 3 / 7",
      "Numpad 5",
    ]) {
      expect(combos).toContain(expected);
    }
  });

  it("shows Ctrl on non-mac and the Maya mouse scheme when selected", () => {
    const pc = shortcutGroups({ preset: "maya", mac: false })
      .flatMap((g) => g.items.map((i) => i.combo))
      .join(" | ");
    expect(pc).toContain("Ctrl Z");
    expect(pc).toContain("Alt LMB");
    expect(pc).not.toContain("\u{2318}");
  });

  it("forwards exactly the shortcut keys from the engine iframe (movement keys stay engine-only)", () => {
    for (const k of ["q", "w", "e", "r", "z", "d", "F5", "?", ".", "Delete", "Backspace", "Escape"]) {
      expect(FORWARDED_KEYS.has(k)).toBe(true);
    }
    for (const k of ["a", "s", "f", "`", " "]) {
      expect(FORWARDED_KEYS.has(k)).toBe(false);
    }
  });

  it("documents the debug step key", () => {
    const items = shortcutGroups({ preset: "blender", mac: true }).flatMap((g) => g.items);
    const step = items.find((i) => i.combo === ".");
    expect(step).toBeTruthy();
    expect(step!.label).toMatch(/step one tick/i);
    expect(step!.label).toMatch(/debug/i);
  });
});
