import { beforeEach, describe, expect, it, vi } from "vitest";

import { registerScene } from "./scene-register.server";
import { FoundryStateError } from "./db.server";

const state = vi.hoisted(() => ({
  hostSids: [] as string[],
  docIds: [] as string[],
  sceneIds: [] as string[],
  scenes: [] as unknown[][],
  changelog: [] as unknown[][],
  actions: [] as unknown[][],
}));

// Enough of Postgres to exercise the write path in memory: getPool/withTx hand
// back one fake query dispatcher, everything else in db.server
// (FoundryStateError included) stays real.
vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  async function query(
    text: string,
    values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    if (text.includes("FROM foundry.gdd_doc")) {
      const found = state.docIds.includes(String(values[0]));
      return { rows: found ? [{ one: 1 }] : [], rowCount: found ? 1 : 0 };
    }
    if (text.includes("INSERT INTO foundry.scene_changelog")) {
      state.changelog.push(values);
      return { rows: [], rowCount: 1 };
    }
    if (text.includes("INSERT INTO foundry.scene")) {
      if (state.sceneIds.includes(String(values[0]))) return { rows: [], rowCount: 0 };
      state.sceneIds.push(String(values[0]));
      state.scenes.push(values);
      return { rows: [{ id: values[0] }], rowCount: 1 };
    }
    if (text.includes("INSERT INTO foundry.action_log")) {
      state.actions.push(values);
      return { rows: [], rowCount: 1 };
    }
    throw new Error(`unexpected query: ${text}`);
  }
  return {
    ...actual,
    getPool: () => ({ query }),
    withTx: async (fn: (client: unknown) => Promise<unknown>) => fn({ query }),
    assertRate: () => {},
  };
});

vi.mock("./roles.server", () => ({
  requireHost: async (_client: unknown, sid: string) => {
    if (!state.hostSids.includes(sid)) {
      throw new FoundryStateError(
        "This needs an active host role with the steward-code consent granted.",
      );
    }
  },
}));

const HOST = "sid-host";

function input(over: Record<string, string> = {}) {
  return {
    sid: HOST,
    id: "lantern-relay",
    title: "Lantern Relay",
    repoPath: "~/one/rig/play/lantern-relay",
    gddDocId: "",
    sourceNote: "hand-registered from the local harness copy",
    ...over,
  };
}

beforeEach(() => {
  state.hostSids = [HOST];
  state.docIds = ["lantern-relay-shortgdd-v1"];
  state.sceneIds = [];
  state.scenes = [];
  state.changelog = [];
  state.actions = [];
});

describe("registerScene", () => {
  it("a host lands a repo row, its changelog entry and the recorded act", async () => {
    const { id } = await registerScene(input());
    expect(id).toBe("lantern-relay");
    expect(state.scenes).toHaveLength(1);
    // (id, title, repo_path, gdd_doc_id, source_note) — source is 'repo' in the
    // SQL text; no mirror column is in the parameter list at all.
    expect(state.scenes[0]).toEqual([
      "lantern-relay",
      "Lantern Relay",
      "~/one/rig/play/lantern-relay",
      null,
      "hand-registered from the local harness copy",
    ]);
    expect(state.changelog).toHaveLength(1);
    expect(state.changelog[0]).toEqual([
      "lantern-relay",
      "Registered on the shelf",
      "hand-registered from the local harness copy",
      HOST,
    ]);
    expect(state.actions).toHaveLength(1);
    expect(state.actions[0]?.[1]).toBe("register_scene");
    expect(state.actions[0]?.[2]).toBe("lantern-relay");
  });

  it("links an existing design doc and refuses a dangling one", async () => {
    await registerScene(input({ gddDocId: "lantern-relay-shortgdd-v1" }));
    expect(state.scenes[0]?.[3]).toBe("lantern-relay-shortgdd-v1");

    await expect(
      registerScene(input({ id: "other-game", gddDocId: "no-such-doc" })),
    ).rejects.toThrow(/No design doc has the id "no-such-doc"/);
    expect(state.sceneIds).toEqual(["lantern-relay"]);
  });

  it("refuses a taken id out loud and writes nothing else", async () => {
    await registerScene(input());
    await expect(registerScene(input({ title: "Another" }))).rejects.toThrow(
      /"lantern-relay" is already on the shelf/,
    );
    expect(state.scenes).toHaveLength(1);
    expect(state.changelog).toHaveLength(1);
    expect(state.actions).toHaveLength(1);
  });

  it("refuses a non-host before anything lands", async () => {
    await expect(registerScene(input({ sid: "sid-visitor" }))).rejects.toThrow(
      /host role/,
    );
    expect(state.scenes).toHaveLength(0);
    expect(state.changelog).toHaveLength(0);
    expect(state.actions).toHaveLength(0);
  });

  it("refuses a malformed id and an empty source note before any query", async () => {
    await expect(registerScene(input({ id: "Bad Slug" }))).rejects.toThrow(
      /lowercase letters, digits and dashes/,
    );
    await expect(registerScene(input({ sourceNote: "  " }))).rejects.toThrow(
      /where this game comes from/,
    );
    await expect(registerScene(input({ title: "" }))).rejects.toThrow(
      /Give the game a title/,
    );
    // The form's own address can never become a game id.
    await expect(registerScene(input({ id: "register" }))).rejects.toThrow(
      /taken by this form/,
    );
    expect(state.scenes).toHaveLength(0);
  });
});
