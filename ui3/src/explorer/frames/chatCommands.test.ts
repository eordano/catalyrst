import { afterEach, describe, expect, test, vi } from "vitest";

import {
  dispatchCommand,
  ENGINE_COMMANDS,
  EPOCH_MS_FLOOR,
  helpText,
  LOCAL_COMMANDS,
  parseCommand,
  parseHelpRow,
  requestEngineHelp,
  wallClockMs,
  type ConsoleLine,
  type ConsoleSource,
} from "./chatCommands";

describe("parseCommand", () => {
  test("plain chat is not a command; a leading slash splits the name and arguments case-insensitively", () => {
    expect(parseCommand("gm nearby")).toBeNull();
    expect(parseCommand("  hello /fps ")).toBeNull();
    expect(parseCommand("/FPS 30")).toEqual({ name: "/fps", args: ["30"] });
    expect(parseCommand("  /teleport -143  102 ")).toEqual({ name: "/teleport", args: ["-143", "102"] });
    expect(parseCommand("/")).toEqual({ name: "/", args: [] });
  });
});

describe("dispatchCommand", () => {
  test("plain text and engine commands go to the engine verbatim; /clear is local; bare /help is a help request", () => {
    expect(dispatchCommand("gm")).toEqual({ kind: "send", message: "gm" });
    expect(dispatchCommand("/fps 30")).toEqual({ kind: "send", message: "/fps 30" });
    expect(dispatchCommand("/reload")).toEqual({ kind: "send", message: "/reload" });
    expect(dispatchCommand("/clear")).toEqual({ kind: "clear" });
    expect(dispatchCommand("/help")).toEqual({ kind: "help" });
  });

  test("/help <command> answers from the curated table and forwards to the engine when the table has no entry", () => {
    expect(dispatchCommand("/help clear")).toEqual({ kind: "print", text: "/clear  \u{2014} clear the chat log on this screen" });
    expect(dispatchCommand("/help fps")).toEqual({ kind: "print", text: "/fps <fps>  \u{2014} set the target frame rate" });
    expect(dispatchCommand("/help /FPS")).toEqual({ kind: "print", text: "/fps <fps>  \u{2014} set the target frame rate" });
    expect(dispatchCommand("/help idnoclip")).toEqual({ kind: "send", message: "/help /idnoclip" });
    expect(dispatchCommand("/help /Scene_Tree")).toEqual({ kind: "send", message: "/help /scene_tree" });
  });
});

describe("helpText", () => {
  test("without engine names it lists the local commands and the whole curated table", () => {
    const text = helpText();
    for (const c of [...LOCAL_COMMANDS, ...ENGINE_COMMANDS]) expect(text).toContain(c.name);
    expect(text.split("\n")).toHaveLength(2 + LOCAL_COMMANDS.length + ENGINE_COMMANDS.length);
  });

  test("with engine names it keeps the local commands on top, then the curated entries the engine reports in table order, then the rest as bare names", () => {
    const rows = helpText(["/asset_catalog", "/clear", "/fps", "/help", "/teleport", "/fps"]).split("\n");
    expect(rows[0]).toBe("Chat commands:");
    expect(rows[1]).toMatch(/^  \/help \[command\]\s+\u{2014} list the chat commands, or the usage of one$/u);
    expect(rows[2]).toMatch(/^  \/clear\s+\u{2014} clear the chat log on this screen$/u);
    expect(rows[3]).toMatch(/^  \/teleport <x> <y> \[realm\]\s+\u{2014} jump to a parcel, optionally in another realm$/u);
    expect(rows[4]).toMatch(/^  \/fps <fps>\s+\u{2014} set the target frame rate$/u);
    expect(rows[5]).toBe("  /asset_catalog");
    expect(rows).toHaveLength(7);
    expect(rows.join("\n")).not.toContain("/reload");
  });
});

describe("parseHelpRow", () => {
  test("reads the command name out of an engine help row and ignores the header, sentinels and free text", () => {
    expect(parseHelpRow("  /fps             - Set the target frame rate")).toBe("/fps");
    expect(parseHelpRow("  /asset_catalog   - ")).toBe("/asset_catalog");
    expect(parseHelpRow("  /jump -")).toBe("/jump");
    expect(parseHelpRow("  exit - leave")).toBe("/exit");
    expect(parseHelpRow("Available commands:")).toBeNull();
    expect(parseHelpRow("[ok]")).toBeNull();
    expect(parseHelpRow("/fps")).toBeNull();
    expect(parseHelpRow("Command '/nope' does not exist")).toBeNull();
  });
});

describe("requestEngineHelp", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  function fakeSource() {
    const listeners = new Set<(line: ConsoleLine) => void>();
    const source: ConsoleSource = (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    };
    const emit = (line: ConsoleLine): void => {
      for (const cb of [...listeners]) cb(line);
    };
    const engine = (message: string, timestamp: number): ConsoleLine => ({ senderAddress: "", senderName: "", message, timestamp });
    return { source, emit, engine, listeners };
  }

  test("sends /help, collects the rows between the header and the sentinel, skips the echo and other senders, then unsubscribes", async () => {
    const { source, emit, engine, listeners } = fakeSource();
    const send = vi.fn();
    const pending = requestEngineHelp(source, send, 1000);
    expect(send).toHaveBeenCalledWith("/help");
    expect(listeners.size).toBe(1);

    emit({ senderAddress: "0xme", senderName: "Me", message: "/help", timestamp: 1 });
    emit(engine("[ok]", 2));
    emit(engine("Available commands:", 3));
    emit(engine("  /fps   - about", 4));
    emit({ senderAddress: "0xother", senderName: "Ripley", message: "  /fake - x", timestamp: 5 });
    emit(engine("  /jump  - ", 6));
    emit(engine("[ok]", 7));
    emit(engine("  /late  - ", 8));

    await expect(pending).resolves.toEqual({
      names: ["/fps", "/jump"],
      keys: ["3|Available commands:", "4|  /fps   - about", "6|  /jump  - ", "7|[ok]"],
    });
    expect(listeners.size).toBe(0);
  });

  test("resolves null and unsubscribes when the sentinel does not arrive within the timeout", async () => {
    vi.useFakeTimers();
    const { source, emit, engine, listeners } = fakeSource();
    const pending = requestEngineHelp(source, vi.fn(), 1500);
    emit(engine("Available commands:", 1));
    emit(engine("  /fps   - about", 2));
    await vi.advanceTimersByTimeAsync(1499);
    expect(listeners.size).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    await expect(pending).resolves.toBeNull();
    expect(listeners.size).toBe(0);
  });
});

describe("wallClockMs", () => {
  const now = Date.UTC(2026, 8, 17, 19, 9);

  test("engine uptime seconds, OLE dates and missing stamps render as now; epoch milliseconds pass through", () => {
    expect(wallClockMs(12.5, now)).toBe(now);
    expect(wallClockMs(46_000.51, now)).toBe(now);
    expect(wallClockMs(0, now)).toBe(now);
    expect(wallClockMs(undefined, now)).toBe(now);
    expect(wallClockMs(Number.NaN, now)).toBe(now);
    expect(wallClockMs(now - 5_000, now)).toBe(now - 5_000);
    expect(wallClockMs(EPOCH_MS_FLOOR, now)).toBe(EPOCH_MS_FLOOR);
  });
});
