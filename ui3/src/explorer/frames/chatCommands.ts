type ChatCommand = { name: string; args?: string; about: string };

export const LOCAL_COMMANDS: ChatCommand[] = [
  { name: "/help", args: "[command]", about: "list the chat commands, or the usage of one" },
  { name: "/clear", about: "clear the chat log on this screen" },
];

export const ENGINE_COMMANDS: ChatCommand[] = [
  { name: "/teleport", args: "<x> <y> [realm]", about: "jump to a parcel, optionally in another realm" },
  { name: "/changerealm", args: "<realm>", about: "connect to another realm or world" },
  { name: "/reload", args: "[scene-hash]", about: "reload the current scene" },
  { name: "/fps", args: "<fps>", about: "set the target frame rate" },
  { name: "/scene_distance", args: "[load] [unload]", about: "set the scene load and unload distance" },
  { name: "/time", args: "[hour] [speed]", about: "set the time of day and how fast it advances" },
  { name: "/emote", args: "<urn|slot>", about: "play an emote by urn or profile slot" },
  { name: "/speed", args: "<walk> <jog> <run>", about: "set the avatar movement speeds" },
  { name: "/jump", args: "<height> <run-height>", about: "set the avatar jump heights" },
  { name: "/shadows", args: "[true|false]", about: "toggle shadows" },
  { name: "/fog", args: "[true|false]", about: "toggle fog" },
  { name: "/bloom", args: "<intensity>", about: "set the bloom intensity" },
  { name: "/show_ui", args: "<scene-hash> [true|false]", about: "show or hide a scene's UI" },
  { name: "/spawn", args: "<ens>", about: "start a portable experience" },
  { name: "/kill", args: "<ens>", about: "stop a portable experience" },
  { name: "/list_portables", about: "list the running portable experiences" },
  { name: "/player_position", about: "print your position" },
  { name: "/connected_players", about: "list the players in the room" },
];

type ParsedCommand = { name: string; args: string[] };

export function parseCommand(message: string): ParsedCommand | null {
  const m = message.trim();
  if (!m.startsWith("/")) return null;
  const [head = "/", ...args] = m.split(/\s+/);
  return { name: head.toLowerCase(), args };
}

function usage(c: ChatCommand): string {
  return c.args ? `${c.name} ${c.args}` : c.name;
}

function describe(c: ChatCommand): string {
  return `${usage(c)}  \u{2014} ${c.about}`;
}

export function helpText(engineNames?: string[]): string {
  const rows: (ChatCommand | string)[] = [...LOCAL_COMMANDS];
  if (engineNames == null) rows.push(...ENGINE_COMMANDS);
  else {
    const reported = new Set(engineNames);
    const listed = new Set(LOCAL_COMMANDS.map((c) => c.name));
    for (const c of ENGINE_COMMANDS) {
      if (!reported.has(c.name) || listed.has(c.name)) continue;
      listed.add(c.name);
      rows.push(c);
    }
    for (const name of engineNames) {
      if (listed.has(name)) continue;
      listed.add(name);
      rows.push(name);
    }
  }
  const width = Math.max(...rows.map((r) => (typeof r === "string" ? r : usage(r)).length));
  const lines = rows.map((r) => (typeof r === "string" ? `  ${r}` : `  ${usage(r).padEnd(width)}  \u{2014} ${r.about}`));
  return ["Chat commands:", ...lines, "Any other /command goes to the engine console; /help <command> prints its usage."].join("\n");
}

type LocalAction = { kind: "clear" } | { kind: "help" } | { kind: "print"; text: string } | { kind: "send"; message: string };

export function dispatchCommand(message: string): LocalAction {
  const cmd = parseCommand(message);
  if (!cmd) return { kind: "send", message };
  if (cmd.name === "/clear") return { kind: "clear" };
  if (cmd.name !== "/help") return { kind: "send", message };
  const [topic] = cmd.args;
  if (!topic) return { kind: "help" };
  const name = topic.startsWith("/") ? topic.toLowerCase() : `/${topic.toLowerCase()}`;
  const known = [...LOCAL_COMMANDS, ...ENGINE_COMMANDS].find((c) => c.name === name);
  return known ? { kind: "print", text: describe(known) } : { kind: "send", message: `/help ${name}` };
}

const HELP_HEADER = "Available commands:";
const HELP_TIMEOUT_MS = 1500;
const HELP_ROW_RE = /^\s+(\S+)\s+-(?:\s.*)?$/;
const SENTINELS = new Set(["[ok]", "[failed]"]);

export type ConsoleLine = { senderAddress?: string; senderName?: string; message?: string; timestamp?: number };
export type ConsoleSource = (listener: (line: ConsoleLine) => void) => () => void;
type EngineHelp = { names: string[]; keys: string[] };

function isEngineLine(line: ConsoleLine): boolean {
  return !line.senderAddress && !line.senderName;
}

export function consoleLineKey(line: ConsoleLine): string {
  return `${line.timestamp ?? ""}|${line.message ?? ""}`;
}

export function parseHelpRow(line: string): string | null {
  const name = HELP_ROW_RE.exec(line)?.[1];
  if (!name) return null;
  return name.startsWith("/") ? name : `/${name}`;
}

export function requestEngineHelp(
  source: ConsoleSource,
  send: (message: string) => void,
  timeoutMs = HELP_TIMEOUT_MS,
): Promise<EngineHelp | null> {
  return new Promise((resolve) => {
    const names: string[] = [];
    const keys: string[] = [];
    let started = false;
    let done = false;
    let unsub: (() => void) | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const finish = (result: EngineHelp | null): void => {
      done = true;
      clearTimeout(timer);
      unsub?.();
      resolve(result);
    };
    unsub = source((line) => {
      if (done || !isEngineLine(line)) return;
      const message = line.message ?? "";
      if (!started) {
        if (message !== HELP_HEADER) return;
        started = true;
        keys.push(consoleLineKey(line));
        return;
      }
      if (SENTINELS.has(message)) {
        keys.push(consoleLineKey(line));
        finish({ names, keys });
        return;
      }
      const name = parseHelpRow(message);
      if (name == null) return;
      names.push(name);
      keys.push(consoleLineKey(line));
    });
    if (done) {
      unsub();
      return;
    }
    timer = setTimeout(() => finish(null), timeoutMs);
    send("/help");
  });
}

export const EPOCH_MS_FLOOR = 1e11;

export function wallClockMs(ts: number | undefined, now: number): number {
  return ts != null && Number.isFinite(ts) && ts >= EPOCH_MS_FLOOR ? ts : now;
}
