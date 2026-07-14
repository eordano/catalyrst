export type DebugSnapshot = Record<string, Record<string, unknown>>;

export interface LeafChange {
  path: string;
  before: string;
  after: string;
}

export type CompChangeKind = "added" | "removed" | "changed";

export interface CompDiff {
  name: string;
  kind: CompChangeKind;
  changes: LeafChange[];
  value?: string;
}

export type EntityChangeKind = "new" | "gone" | "changed";

export interface EntityDiff {
  id: string;
  kind: EntityChangeKind;
  comps: CompDiff[];
  unchanged: number;
}

export function formatNum(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  if (Number.isInteger(n)) return String(n);
  const r = Math.round(n * 1000) / 1000;
  return String(r);
}

const MAX_STR = 42;

export function formatValue(v: unknown, depth = 0): string {
  if (v === null) return "null";
  if (v === undefined) return "\u{2205}";
  switch (typeof v) {
    case "number":
      return formatNum(v);
    case "boolean":
      return String(v);
    case "string":
      return v.length > MAX_STR ? JSON.stringify(v.slice(0, MAX_STR - 1) + "\u{2026}") : JSON.stringify(v);
    case "object":
      break;
    default:
      return String(v);
  }
  if (depth >= 2) return Array.isArray(v) ? `[\u{2026}${(v as unknown[]).length}]` : "{\u{2026}}";
  if (Array.isArray(v)) {
    const shown = v.slice(0, 4).map((x) => formatValue(x, depth + 1));
    const extra = v.length > 4 ? `, \u{2026}+${v.length - 4}` : "";
    return `[${shown.join(", ")}${extra}]`;
  }
  const entries = Object.entries(v as Record<string, unknown>);
  const shown = entries.slice(0, 6).map(([k, x]) => `${k}: ${formatValue(x, depth + 1)}`);
  const extra = entries.length > 6 ? `, \u{2026}+${entries.length - 6}` : "";
  return `{${shown.join(", ")}${extra}}`;
}

export function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  const aArr = Array.isArray(a);
  if (aArr !== Array.isArray(b)) return false;
  if (aArr) {
    const x = a as unknown[];
    const y = b as unknown[];
    if (x.length !== y.length) return false;
    return x.every((v, i) => deepEqual(v, y[i]));
  }
  const ao = a as Record<string, unknown>;
  const bo = b as Record<string, unknown>;
  const keys = Object.keys(ao);
  if (keys.length !== Object.keys(bo).length) return false;
  return keys.every((k) => k in bo && deepEqual(ao[k], bo[k]));
}

export const MAX_LEAF_CHANGES = 12;

export function leafChanges(before: unknown, after: unknown, cap = MAX_LEAF_CHANGES): LeafChange[] {
  const out: LeafChange[] = [];
  const walk = (a: unknown, b: unknown, path: string): void => {
    if (out.length >= cap) return;
    if (deepEqual(a, b)) return;
    const bothObj =
      a !== null &&
      b !== null &&
      typeof a === "object" &&
      typeof b === "object" &&
      Array.isArray(a) === Array.isArray(b) &&
      !Array.isArray(a);
    if (!bothObj) {
      out.push({ path, before: formatValue(a), after: formatValue(b) });
      return;
    }
    const ao = a as Record<string, unknown>;
    const bo = b as Record<string, unknown>;
    for (const k of new Set([...Object.keys(ao), ...Object.keys(bo)])) {
      walk(ao[k], bo[k], path === "" ? k : `${path}.${k}`);
      if (out.length >= cap) return;
    }
  };
  walk(before, after, "");
  return out;
}

export function diffSnapshots(prev: DebugSnapshot, next: DebugSnapshot): EntityDiff[] {
  const out: EntityDiff[] = [];
  const ids = new Set([...Object.keys(prev), ...Object.keys(next)]);
  for (const id of ids) {
    const a = prev[id];
    const b = next[id];
    if (a === undefined && b === undefined) continue;
    if (a === undefined) {
      out.push({
        id,
        kind: "new",
        comps: Object.entries(b!).map(([name, v]) => ({
          name,
          kind: "added" as const,
          changes: [],
          value: formatValue(v),
        })),
        unchanged: 0,
      });
      continue;
    }
    if (b === undefined) {
      out.push({
        id,
        kind: "gone",
        comps: Object.keys(a).map((name) => ({ name, kind: "removed" as const, changes: [] })),
        unchanged: 0,
      });
      continue;
    }
    const comps: CompDiff[] = [];
    let unchanged = 0;
    for (const name of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const va = a[name];
      const vb = b[name];
      if (va === undefined) {
        comps.push({ name, kind: "added", changes: [], value: formatValue(vb) });
      } else if (vb === undefined) {
        comps.push({ name, kind: "removed", changes: [], value: formatValue(va) });
      } else if (deepEqual(va, vb)) {
        unchanged += 1;
      } else {
        comps.push({ name, kind: "changed", changes: leafChanges(va, vb) });
      }
    }
    if (comps.length > 0) out.push({ id, kind: "changed", comps, unchanged });
  }
  return orderEntityDiffs(out);
}

const RESERVED = new Set(["0", "1", "2"]);

export function orderEntityDiffs(diffs: EntityDiff[]): EntityDiff[] {
  const rank = (d: EntityDiff): number => (RESERVED.has(d.id) ? 1 : 0);
  return diffs
    .slice()
    .sort((a, b) => rank(a) - rank(b) || Number(a.id) - Number(b.id));
}

export function snapshotEntityCount(s: DebugSnapshot): number {
  return Object.keys(s).length;
}

export const DEBUG_LOG_PREFIX = "one-dbg ";

export interface DebugSystemRow {
  name: string;
  ran: boolean;
  runs: number;
}

export interface DebugManifest {
  game: string | null;
  tick: number;
  systems: DebugSystemRow[];
  created: number[];
  handlers: number;
  registry?: boolean;
}

export interface DebugSystemsView {
  game: string | null;
  rows: DebugSystemRow[];
  handlers: number;
  harnessTick: number | null;
}

export function parseDebugLogManifests(raw: string): DebugManifest[] {
  const out: DebugManifest[] = [];
  for (const line of raw.split("\n")) {
    const i = line.indexOf(DEBUG_LOG_PREFIX);
    if (i === -1) continue;
    try {
      const m = JSON.parse(line.slice(i + DEBUG_LOG_PREFIX.length)) as DebugManifest;
      if (m && typeof m === "object" && typeof m.tick === "number" && Array.isArray(m.systems)) {
        out.push(m);
      }
    } catch {
    }
  }
  return out;
}

export function aggregateManifests(
  batch: DebugManifest[],
  fallback: DebugManifest | null,
): DebugSystemsView | null {
  const source = batch.length > 0 ? batch : fallback !== null ? [fallback] : [];
  const last = source[source.length - 1];
  if (last === undefined) return null;
  const ranByName = new Map<string, boolean>();
  for (const m of source) {
    if (m.registry) continue;
    for (const s of m.systems) {
      ranByName.set(s.name, (ranByName.get(s.name) ?? false) || s.ran);
    }
  }
  const rows: DebugSystemRow[] = last.systems.map((s) => ({
    name: s.name,
    ran: ranByName.get(s.name) ?? false,
    runs: s.runs,
  }));
  for (const [name, ran] of ranByName) {
    if (!rows.some((r) => r.name === name)) rows.push({ name, ran, runs: 0 });
  }
  return {
    game: last.game,
    rows,
    handlers: last.handlers,
    harnessTick: last.registry ? null : last.tick,
  };
}

export function parseTickReply(reply: string): number | null {
  const m = /from\s+(\d+)/.exec(reply);
  return m ? Number(m[1]) : null;
}

export interface SceneStats {
  tick: number | null;
  frozen: boolean;
  title: string | null;
  entities: number | null;
}

export function parseSceneStats(raw: string): SceneStats {
  const tick = /tick:\s*(\d+)/.exec(raw);
  const entities = /entities:\s*(\d+)/.exec(raw);
  const status = /status:\s*(.+)/.exec(raw);
  const title = /scene:\s*'([^']*)'/.exec(raw);
  return {
    tick: tick ? Number(tick[1]) : null,
    frozen: status ? /frozen/i.test(status[1] ?? "") : false,
    title: title?.[1] ?? null,
    entities: entities ? Number(entities[1]) : null,
  };
}

export type ConsoleRun = (cmd: string) => Promise<string>;

export interface StepDebuggerOpts {
  run: ConsoleRun;
  sleep?: (ms: number) => Promise<void>;
  pollIntervalMs?: number;
  maxPollMs?: number;
}

export interface DebugOpenResult {
  tick: number | null;
  entities: number;
  frozen: boolean;
  systems: DebugSystemsView | null;
}

export interface DebugStepResult {
  tick: number | null;
  fromTick: number | null;
  count: number;
  entries: EntityDiff[];
  unchangedEntities: number;
  totalEntities: number;
  timedOut: boolean;
  systems: DebugSystemsView | null;
}

export interface StepDebugger {
  open(): Promise<DebugOpenResult>;
  step(count: number): Promise<DebugStepResult | null>;
  isBusy(): boolean;
  dispose(): void;
}

const defaultSleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export function createStepDebugger(opts: StepDebuggerOpts): StepDebugger {
  const run = opts.run;
  const sleep = opts.sleep ?? defaultSleep;
  const pollMs = opts.pollIntervalMs ?? 60;
  const budgetMs = opts.maxPollMs ?? 4_000;

  let prev: DebugSnapshot | null = null;
  let busy = false;
  let disposed = false;

  const seenManifests = new Set<string>();
  let latestManifest: DebugManifest | null = null;

  const readTelemetry = async (): Promise<DebugSystemsView | null> => {
    let raw = "";
    try {
      raw = await run("scene_logs 150");
    } catch {
      return aggregateManifests([], latestManifest);
    }
    const fresh: DebugManifest[] = [];
    for (const m of parseDebugLogManifests(raw)) {
      const key = `${m.game}|${m.tick}|${m.registry ? 1 : 0}`;
      if (seenManifests.has(key)) continue;
      seenManifests.add(key);
      latestManifest = m;
      fresh.push(m);
    }
    return aggregateManifests(fresh, latestManifest);
  };

  const snapshot = async (): Promise<DebugSnapshot> => {
    const raw = await run("crdt_snapshot");
    const parsed = JSON.parse(raw) as unknown;
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error("crdt_snapshot: unexpected shape");
    }
    return parsed as DebugSnapshot;
  };

  const stats = async (): Promise<SceneStats> => parseSceneStats(await run("scene_stats"));

  const waitFrozen = async (target: number | null, budget: number): Promise<{ st: SceneStats; timedOut: boolean }> => {
    let last: SceneStats = { tick: null, frozen: false, title: null, entities: null };
    const rounds = Math.max(1, Math.ceil(budget / pollMs));
    for (let i = 0; i < rounds; i += 1) {
      if (disposed) break;
      last = await stats();
      if (last.frozen && (target === null || (last.tick !== null && last.tick >= target))) {
        return { st: last, timedOut: false };
      }
      await sleep(pollMs);
    }
    return { st: last, timedOut: true };
  };

  return {
    async open() {
      let { st, timedOut } = await waitFrozen(null, 1_200);
      if (timedOut && !disposed) {
        await run("freeze_scene").catch(() => undefined);
        ({ st, timedOut } = await waitFrozen(null, 1_200));
      }
      prev = await snapshot();
      const systems = await readTelemetry();
      return { tick: st.tick, entities: snapshotEntityCount(prev), frozen: st.frozen, systems };
    },

    async step(count: number) {
      if (busy || disposed) return null;
      const n = Math.max(1, Math.floor(count) || 1);
      busy = true;
      try {
        if (prev === null) prev = await snapshot();
        let reply: string;
        try {
          reply = await run(`tick_scene ${n}`);
        } catch (e) {
          if (!/not frozen/i.test(String(e))) throw e;
          await run("freeze_scene").catch(() => undefined);
          reply = await run(`tick_scene ${n}`);
        }
        const fromTick = parseTickReply(reply);
        const target = fromTick === null ? null : fromTick + n;
        const { st, timedOut } = await waitFrozen(target, budgetMs + n * 50);
        const next = await snapshot();
        const systems = await readTelemetry();
        const entries = diffSnapshots(prev, next);
        const total = snapshotEntityCount(next);
        const changed = entries.filter((d) => d.kind !== "gone").length;
        prev = next;
        return {
          tick: st.tick ?? target,
          fromTick,
          count: n,
          entries,
          unchangedEntities: Math.max(0, total - changed),
          totalEntities: total,
          timedOut,
          systems,
        };
      } finally {
        busy = false;
      }
    },

    isBusy: () => busy,
    dispose() {
      disposed = true;
      prev = null;
    },
  };
}

export function engineConsoleRunner(frame: HTMLIFrameElement | null): ConsoleRun | null {
  try {
    const w = frame?.contentWindow as
      | (Window & { engine_console_command?: (c: string) => Promise<string> })
      | null
      | undefined;
    const fn = w?.engine_console_command;
    if (!w || typeof fn !== "function") return null;
    return (cmd: string) => Promise.resolve(fn.call(w, cmd)).then((r) => String(r));
  } catch {
    return null;
  }
}
