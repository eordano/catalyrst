import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import matter from "gray-matter";

import { StoryMetaSchema, type StoryMeta } from "../packages/core/src/lib/experiments/context";
import { sampleSize } from "./sample-size";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const STORIES_DIR = path.resolve(HERE, "../packages/features/src/stories");

type Args = {
  id: string;
  slug: string;
  owner: string;
  key: string;
  baseline: number;
  mde: number;
  multi: boolean;
  force: boolean;
};

function parseArgs(argv: string[]): Args {
  const flags: Record<string, string | true> = {};
  const positional: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next !== undefined && !next.startsWith("--")) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = true;
      }
    } else {
      positional.push(a);
    }
  }

  const id = (flags.id as string) ?? positional[0];
  if (!id || typeof id !== "string") {
    throw new Error("missing <id> (the story slug, e.g. `places-hero` or `misc/places-hero`)");
  }
  const segments = id.split("/");
  if (!segments.every((s) => /^[a-z0-9][a-z0-9-]*$/.test(s))) {
    throw new Error(
      `invalid id "${id}": each slash-separated segment must use lowercase letters, digits and dashes`,
    );
  }
  const slug = segments[segments.length - 1];

  const num = (v: string | true | undefined, dflt: number): number => {
    if (v === undefined || v === true) return dflt;
    const n = Number(v);
    return Number.isFinite(n) ? n : dflt;
  };

  return {
    id,
    slug,
    owner: (flags.owner as string) ?? "owner@example.com",
    key: (flags.key as string) ?? slug.replace(/-/g, "_"),
    baseline: num(flags.baseline, 0.1),
    mde: num(flags.mde, 0.02),
    multi: flags.multi === true || flags.multi === "true",
    force: flags.force === true || flags.force === "true",
  };
}

function buildMeta(args: Args, minSample: number): StoryMeta {
  return {
    id: args.slug,
    status: "draft",
    owner: args.owner,
    hypothesis: {
      statement: `TODO: state the change and the expected effect for "${args.id}".`,
      because: "TODO: explain the user behaviour / mechanism behind the hypothesis.",
    },
    metric: {
      primary: `${args.key}_rate`,
      guardrails: [],
    },
    experiment: {
      key: args.key,
      unit: "session",
      variants: [
        { id: "control", weight: 50, flags: {} },
        { id: "treatment", weight: 50, flags: {} },
      ],
      baseline: args.baseline,
      mde: args.mde,
      min_sample: minSample,
    },
    decision: {
      rule:
        `Ship treatment if ${args.key}_rate beats control with 95% confidence and ` +
        `no guardrail regresses; otherwise keep control.`,
    },
  };
}

function renderStoryMd(meta: StoryMeta, multi: boolean): string {
  StoryMetaSchema.parse(meta);

  const body = multi
    ? [
        `# Story \u{2014} ${meta.id}`,
        "",
        "Multi-step flow. The XState `machine.ts` drives the steps; the surface",
        `component renders them and fires telemetry on each meaningful transition.`,
        "",
        "- **control** \u{2014} baseline flow.",
        "- **treatment** \u{2014} the changed flow under test.",
        "",
        "Fill in the machine states, the component, and the metric/guardrail",
        "definitions above, then run `npm run story:readout -- " + meta.id + "`.",
      ].join("\n")
    : [
        `# Story \u{2014} ${meta.id}`,
        "",
        "Single-surface experiment. Resolve the assignment in the route loader,",
        "render the treatment behind its flag, and fire the exposure event when",
        "the experiment surface renders.",
        "",
        "- **Primary metric:** `" + meta.metric.primary + "`.",
        "- **Guardrails:** TODO.",
        "",
        "Run `npm run story:readout -- " + meta.id + "` to evaluate.",
      ].join("\n");

  return matter.stringify(`\n${body}\n`, meta as unknown as Record<string, unknown>);
}

function pascal(id: string): string {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((s) => s[0].toUpperCase() + s.slice(1))
    .join("");
}

function renderMachineTs(comp: string): string {
  return `// XState v5 statechart for "${comp}". Fill in states + telemetry actions.
//
// Effects (launch resolvers, telemetry) are injectable through machine \`input\`
// so model-based tests can stub them and never touch the network. Mirror the
// jump-in machine for the established pattern.

import { setup } from "xstate";

import { track as defaultTrack, type TrackContext } from "../../lib/telemetry/track";

export type ${comp}Input = {
  /** Telemetry context (sid/story/variant/experimentKey). */
  trackCtx: TrackContext;
  /** Resolved per-variant flags. */
  flags: Record<string, unknown>;
  /** Injectable telemetry sink; defaults to the real \`track\`. */
  track?: typeof defaultTrack;
};

export type ${comp}Context = {
  trackCtx: TrackContext;
  flags: Record<string, unknown>;
  track: typeof defaultTrack;
};

export type ${comp}Event = { type: "START" } | { type: "DONE" };

/** Canonical telemetry event names emitted by the flow. */
export const ${comp.toUpperCase()}_EVENTS = {
  started: "${comp.toLowerCase()}_started",
  completed: "${comp.toLowerCase()}_completed",
} as const;

export const ${comp.charAt(0).toLowerCase() + comp.slice(1)}Machine = setup({
  types: {
    context: {} as ${comp}Context,
    events: {} as ${comp}Event,
    input: {} as ${comp}Input,
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(${comp.toUpperCase()}_EVENTS.started, {}, context.trackCtx),
    trackCompleted: ({ context }) =>
      context.track(${comp.toUpperCase()}_EVENTS.completed, {}, context.trackCtx),
  },
}).createMachine({
  id: "${comp.charAt(0).toLowerCase() + comp.slice(1)}",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    flags: input.flags,
    track: input.track ?? defaultTrack,
  }),
  initial: "idle",
  states: {
    idle: { on: { START: { target: "active", actions: "trackStarted" } } },
    active: { on: { DONE: { target: "done", actions: "trackCompleted" } } },
    done: { type: "final" },
  },
});

export type ${comp}Machine = typeof ${comp.charAt(0).toLowerCase() + comp.slice(1)}Machine;
`;
}

function renderComponentTsx(comp: string): string {
  const machineVar = comp.charAt(0).toLowerCase() + comp.slice(1) + "Machine";
  return `// ${comp} \u{2014} interactive surface driving the \`${machineVar}\`.
//
// Renders the multi-step flow and fires the exposure event once the experiment
// surface mounts. Prefer @ui atoms/molecules for the visuals (do NOT edit ui3).

import { useEffect } from "react";
import { useMachine } from "@xstate/react";

import type { TrackContext } from "../../lib/telemetry/track";
import { trackExposure } from "../../lib/telemetry/track";
import { ${machineVar} } from "./machine";

export type ${comp}Props = {
  variant: string;
  flags: Record<string, unknown>;
  trackCtx: TrackContext;
};

export default function ${comp}({ variant, flags, trackCtx }: ${comp}Props) {
  const [state, send] = useMachine(${machineVar}, { input: { trackCtx, flags } });

  // Exposure fires once, when the experiment surface renders.
  useEffect(() => {
    trackExposure(trackCtx);
  }, [trackCtx]);

  return (
    <div className="${comp.toLowerCase()}" data-variant={variant}>
      <button type="button" onClick={() => send({ type: "START" })}>
        START
      </button>
      <p>state: {String(state.value)}</p>
    </div>
  );
}
`;
}

function renderMachineTestTs(comp: string): string {
  const machineVar = comp.charAt(0).toLowerCase() + comp.slice(1) + "Machine";
  return `// Model-based + execution tests for the ${comp} statechart.
//
// 1. @xstate/graph getShortestPaths() enumerates reachable paths (deterministic;
//    telemetry stubbed via input).
// 2. Run the machine and assert the exact telemetry events fire.

import { describe, expect, it, vi } from "vitest";
import { createActor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import type { track } from "../../lib/telemetry/track";
import { ${machineVar}, ${comp.toUpperCase()}_EVENTS, type ${comp}Input } from "./machine";

function inputFor(trackFn: typeof track): ${comp}Input {
  return {
    trackCtx: {
      sid: "sid-test",
      story: "${comp.toLowerCase()}",
      variant: "treatment",
      experimentKey: "${comp.toLowerCase()}",
    },
    flags: {},
    track: trackFn,
  };
}

describe("${machineVar} \u{2014} model-based path coverage", () => {
  it("every reachable path ends in a known state", () => {
    const paths = getShortestPaths(${machineVar}, { input: inputFor(() => {}) });
    expect(paths.length).toBeGreaterThan(0);
    const known = new Set(["idle", "active", "done"]);
    for (const p of paths) expect(known.has(p.state.value as string)).toBe(true);
  });
});

describe("${machineVar} \u{2014} telemetry events", () => {
  it("START then DONE fires started + completed", () => {
    const track = vi.fn();
    const actor = createActor(${machineVar}, { input: inputFor(track) }).start();
    actor.send({ type: "START" });
    actor.send({ type: "DONE" });
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(${comp.toUpperCase()}_EVENTS.started);
    expect(events).toContain(${comp.toUpperCase()}_EVENTS.completed);
  });
});
`;
}

function writeFileSafe(file: string, contents: string, force: boolean): void {
  if (fs.existsSync(file) && !force) {
    throw new Error(`refusing to overwrite existing file (use --force): ${file}`);
  }
  fs.writeFileSync(file, contents, "utf8");
}

function main(): void {
  const args = parseArgs(process.argv.slice(2));
  const storyDir = path.join(STORIES_DIR, args.id);

  if (fs.existsSync(storyDir) && !args.force) {
    throw new Error(
      `story "${args.id}" already exists at ${storyDir} (use --force to overwrite files)`,
    );
  }
  fs.mkdirSync(storyDir, { recursive: true });

  const ss = sampleSize({ baseline: args.baseline, mde: args.mde });
  const meta = buildMeta(args, ss.perVariant);
  const storyMd = renderStoryMd(meta, args.multi);
  writeFileSafe(path.join(storyDir, "story.md"), storyMd, args.force);

  const written = [path.join(storyDir, "story.md")];

  if (args.multi) {
    const comp = pascal(args.slug);
    const machine = path.join(storyDir, "machine.ts");
    const component = path.join(storyDir, `${comp}.tsx`);
    const test = path.join(storyDir, "machine.test.ts");
    writeFileSafe(machine, renderMachineTs(comp), args.force);
    writeFileSafe(component, renderComponentTsx(comp), args.force);
    writeFileSafe(test, renderMachineTestTs(comp), args.force);
    written.push(machine, component, test);
  }

  process.stderr.write(
    `created story "${args.id}"\n` +
      `  baseline=${args.baseline}  mde=${args.mde}  ` +
      `min_sample (per variant)=${ss.perVariant}\n` +
      written.map((f) => `  + ${path.relative(process.cwd(), f)}`).join("\n") +
      "\n\nNext: edit story.md (hypothesis, metric, guardrails, flags), then\n" +
      `  npm run story:readout -- ${args.id}   # evaluate once data lands\n`,
  );
  process.stdout.write(
    JSON.stringify({ id: args.id, dir: storyDir, minSample: ss.perVariant, files: written }) +
      "\n",
  );
}

function isMain(): boolean {
  try {
    const entry = process.argv[1] ?? "";
    return import.meta.url === `file://${entry}` || entry.endsWith("story-new.ts");
  } catch {
    return false;
  }
}

if (isMain()) {
  try {
    main();
  } catch (err) {
    process.stderr.write(
      `story:new error: ${err instanceof Error ? err.message : String(err)}\n` +
        `usage: npm run story:new -- <id> [--multi] [--owner <email>] [--key <flag>] ` +
        `[--baseline <0..1>] [--mde <abs diff>] [--force]\n` +
        `  <id> is flat ("places-hero") or nested ("misc/places-hero"); each ` +
        `slash-separated segment uses lowercase letters, digits and dashes\n`,
    );
    process.exit(1);
  }
}
