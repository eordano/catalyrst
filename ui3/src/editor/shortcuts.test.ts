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
  it("select a tool in the static edit camera only: never while playing, never in the fly camera, never with a modifier", () => {
    for (const [k, tool] of [
      ["q", "select"],
      ["w", "translate"],
      ["e", "rotate"],
      ["r", "scale"],
    ] as const) {
      expect(shortcutActionFor(key({ key: k }), ctx())).toEqual({ type: "tool", tool });
      expect(shortcutActionFor(key({ key: k }), ctx({ camMode: "none" }))).toEqual({
        type: "tool",
        tool,
      });
      expect(shortcutActionFor(key({ key: k }), ctx({ playing: true }))).toBeNull();
      expect(shortcutActionFor(key({ key: k }), ctx({ camMode: "free" }))).toBeNull();
    }
    expect(shortcutActionFor(key({ key: "w", metaKey: true }), ctx())).toBeNull();
    expect(shortcutActionFor(key({ key: "e", altKey: true }), ctx())).toBeNull();
  });
});

describe("edit keys", () => {
  it("undo/redo work in every camera state even while playing; duplicate and delete need their exact chords", () => {
    for (const c of [ctx(), ctx({ camMode: "free" }), ctx({ playing: true })]) {
      expect(shortcutActionFor(key({ key: "z", metaKey: true }), c)).toEqual({ type: "undo" });
      expect(shortcutActionFor(key({ key: "z", ctrlKey: true }), c)).toEqual({ type: "undo" });
      expect(shortcutActionFor(key({ key: "z", metaKey: true, shiftKey: true }), c)).toEqual({
        type: "redo",
      });
    }
    expect(shortcutActionFor(key({ key: "d", metaKey: true }), ctx())).toEqual({ type: "duplicate" });
    expect(shortcutActionFor(key({ key: "d", ctrlKey: true }), ctx())).toEqual({ type: "duplicate" });
    expect(shortcutActionFor(key({ key: "d" }), ctx())).toBeNull();
    expect(shortcutActionFor(key({ key: "Delete" }), ctx())).toEqual({ type: "delete" });
    expect(shortcutActionFor(key({ key: "Backspace" }), ctx())).toEqual({ type: "delete" });
    expect(shortcutActionFor(key({ key: "Delete", ctrlKey: true }), ctx())).toBeNull();
  });
});

describe("control keys", () => {
  it("F5 plays, ? toggles the overlay, Esc clears the selection; while the overlay is open ? and Esc close it and everything else stands down", () => {
    expect(shortcutActionFor(key({ key: "F5" }), ctx())).toEqual({ type: "play" });
    expect(shortcutActionFor(key({ key: "?", shiftKey: true }), ctx())).toEqual({
      type: "toggle-overlay",
    });
    expect(shortcutActionFor(key({ key: "Escape" }), ctx())).toEqual({
      type: "clear-selection",
    });
    const open = ctx({ overlayOpen: true });
    expect(shortcutActionFor(key({ key: "?" }), open)).toEqual({ type: "close-overlay" });
    expect(shortcutActionFor(key({ key: "Escape" }), open)).toEqual({ type: "close-overlay" });
    expect(shortcutActionFor(key({ key: "w" }), open)).toBeNull();
    expect(shortcutActionFor(key({ key: "Delete" }), open)).toBeNull();
  });

  it("menus/modals take priority over everything, and '.' steps one tick only while the debug panel is open", () => {
    const modal = ctx({ menuOrModalOpen: true });
    for (const k of ["Escape", "w", "Delete", "F5", "?"]) {
      expect(shortcutActionFor(key({ key: k }), modal)).toBeNull();
    }
    expect(shortcutActionFor(key({ key: "z", metaKey: true }), modal)).toBeNull();

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
  const dispatchOn = (el: HTMLElement, k: string): unknown => {
    document.body.appendChild(el);
    let seen: unknown = "unset";
    const onKey = (e: Event) => {
      seen = shortcutActionFor(e as KeyboardEvent, ctx());
    };
    window.addEventListener("keydown", onKey, { capture: true });
    el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
    window.removeEventListener("keydown", onKey, { capture: true });
    el.remove();
    return seen;
  };

  it("suppresses shortcuts typed into inputs, textareas, selects and contentEditable surfaces, but not on plain buttons or the window", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(dispatchOn(document.createElement(tag), "w"), tag).toBeNull();
    }
    expect(
      isTypingTarget({
        composedPath: () => [{ tagName: "DIV", isContentEditable: true } as unknown as EventTarget],
        target: null,
      }),
    ).toBe(true);
    expect(dispatchOn(document.createElement("button"), "q")).toEqual({ type: "tool", tool: "select" });
    expect(shortcutActionFor(key({ key: "q" }), ctx())).toEqual({ type: "tool", tool: "select" });
  });

  it("leaves modern EditContext and nested textbox editing to the code editor", () => {
    for (const attribute of [{ role: "textbox" }, { class: "monaco-editor" }]) {
      const surface = document.createElement("div");
      for (const [name, value] of Object.entries(attribute)) surface.setAttribute(name, value);
      const target = document.createElement("div");
      surface.appendChild(target);
      for (const key of ["q", "w", "e", "r", "Delete", "Backspace", "Escape", "F5"]) {
        document.body.appendChild(surface);
        let action: unknown;
        const listener = (event: KeyboardEvent) => { action = shortcutActionFor(event, ctx()); };
        window.addEventListener("keydown", listener, true);
        target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
        window.removeEventListener("keydown", listener, true);
        surface.remove();
        expect(action, `${JSON.stringify(attribute)} ${key}`).toBeNull();
      }
    }
  });
});

describe("cheatsheet completeness", () => {
  it("documents every discrete binding plus the mouse scheme, views, backtick, F and the debug step key, in the selected preset's spelling", () => {
    const mac = shortcutGroups({ preset: "blender", mac: true });
    const combos = mac.flatMap((g) => g.items.map((i) => i.combo)).join(" | ");
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
    const step = mac.flatMap((g) => g.items).find((i) => i.combo === ".");
    expect(step!.label).toMatch(/step one tick/i);
    expect(step!.label).toMatch(/debug/i);

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
});
