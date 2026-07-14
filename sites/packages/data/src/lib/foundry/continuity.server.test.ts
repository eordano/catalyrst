import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  acceptTransfer,
  addSceneNote,
  claimSteward,
  listSceneMemory,
  listStewards,
  listTransfers,
  offerTransfer,
  releaseSteward,
  revokeTransfer,
} from "./continuity.server";
import { FoundryStateError } from "./db.server";

type StewardRec = {
  scene_id: string;
  sid: string;
  basis: string;
  via_transfer_id: string | null;
  since: Date;
  released_at: Date | null;
  release_reason: string | null;
};

type TransferRec = {
  id: string;
  scene_id: string;
  from_sid: string;
  token_hash: string;
  note: string;
  status: string;
  created_at: Date;
  expires_at: Date;
  accepted_sid: string | null;
  accepted_at: Date | null;
};

const state = vi.hoisted(() => ({
  stewards: [] as StewardRec[],
  transfers: [] as TransferRec[],
  actions: [] as { sid: string; action: string; subject: string }[],
  changelog: [] as { scene_id: string; note: string; sid: string }[],
  personas: [] as { sid: string; display_name: string }[],
  nextTransferId: 1,
}));

// Enough of Postgres to exercise the steward/transfer state machine in memory:
// the partial unique index on active seats, the status-guarded transfer
// UPDATEs, and the accept transaction's three writes. getPool/withTx hand back
// one fake query dispatcher; FoundryStateError and sidBadge stay real.
vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  async function query(
    text: string,
    values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    if (text.includes("SELECT 1 AS one FROM foundry.scene_steward")) {
      const found = state.stewards.some(
        (s) =>
          s.scene_id === values[0] && s.sid === values[1] && s.released_at === null,
      );
      return { rows: found ? [{ one: 1 }] : [], rowCount: found ? 1 : 0 };
    }
    if (
      text.includes("INSERT INTO foundry.scene_steward") &&
      text.includes("via_transfer_id")
    ) {
      const [scene_id, sid, basis, via] = values as [string, string, string, string];
      const active = state.stewards.some(
        (s) => s.scene_id === scene_id && s.sid === sid && s.released_at === null,
      );
      if (active) return { rows: [], rowCount: 0 };
      state.stewards.push({
        scene_id,
        sid,
        basis,
        via_transfer_id: via,
        since: new Date(),
        released_at: null,
        release_reason: null,
      });
      return { rows: [], rowCount: 1 };
    }
    if (text.includes("INSERT INTO foundry.scene_steward")) {
      const [scene_id, sid, basis] = values as [string, string, string];
      const active = state.stewards.some(
        (s) => s.scene_id === scene_id && s.sid === sid && s.released_at === null,
      );
      if (active) return { rows: [], rowCount: 0 };
      state.stewards.push({
        scene_id,
        sid,
        basis,
        via_transfer_id: null,
        since: new Date(),
        released_at: null,
        release_reason: null,
      });
      return { rows: [], rowCount: 1 };
    }
    if (
      text.includes("UPDATE foundry.scene_steward") &&
      text.includes("release_reason = 'self'")
    ) {
      const [scene_id, sid] = values as [string, string];
      const row = state.stewards.find(
        (s) => s.scene_id === scene_id && s.sid === sid && s.released_at === null,
      );
      if (!row) return { rows: [], rowCount: 0 };
      row.released_at = new Date();
      row.release_reason = "self";
      return { rows: [], rowCount: 1 };
    }
    if (
      text.includes("UPDATE foundry.scene_steward") &&
      text.includes("release_reason = 'transfer'")
    ) {
      const [scene_id, sid] = values as [string, string];
      const row = state.stewards.find(
        (s) => s.scene_id === scene_id && s.sid === sid && s.released_at === null,
      );
      if (!row) return { rows: [], rowCount: 0 };
      row.released_at = new Date();
      row.release_reason = "transfer";
      return { rows: [], rowCount: 1 };
    }
    if (text.includes("SELECT sid, basis, since, released_at")) {
      const rows = state.stewards
        .filter((s) => s.scene_id === values[0])
        .sort((a, b) => {
          const act = Number(b.released_at === null) - Number(a.released_at === null);
          return act !== 0 ? act : a.since.getTime() - b.since.getTime();
        });
      return { rows, rowCount: rows.length };
    }
    if (text.includes("INSERT INTO foundry.scene_transfer")) {
      const [scene_id, from_sid, token_hash, note, ttl] = values as [
        string,
        string,
        string,
        string,
        string,
      ];
      const id = `tr-${state.nextTransferId++}`;
      state.transfers.push({
        id,
        scene_id,
        from_sid,
        token_hash,
        note,
        status: "offered",
        created_at: new Date(),
        expires_at: new Date(Date.now() + Number(ttl)),
        accepted_sid: null,
        accepted_at: null,
      });
      return { rows: [{ id }], rowCount: 1 };
    }
    if (text.includes("UPDATE foundry.scene_transfer SET status = 'revoked'")) {
      const [id, sid] = values as [string, string];
      const row = state.transfers.find(
        (t) => t.id === id && t.from_sid === sid && t.status === "offered",
      );
      if (!row) return { rows: [], rowCount: 0 };
      row.status = "revoked";
      return { rows: [{ scene_id: row.scene_id }], rowCount: 1 };
    }
    if (
      text.includes("SELECT id, scene_id, from_sid, status, expires_at") &&
      text.includes("token_hash")
    ) {
      const rows = state.transfers.filter((t) => t.token_hash === values[0]);
      return { rows, rowCount: rows.length };
    }
    if (text.includes("SET status = 'accepted'")) {
      const [id, sid] = values as [string, string];
      const row = state.transfers.find((t) => t.id === id);
      if (!row) return { rows: [], rowCount: 0 };
      row.status = "accepted";
      row.accepted_sid = sid;
      row.accepted_at = new Date();
      return { rows: [], rowCount: 1 };
    }
    if (
      text.includes("FROM foundry.scene_transfer") &&
      text.includes("ORDER BY created_at DESC")
    ) {
      const rows = state.transfers
        .filter((t) => t.scene_id === values[0])
        .sort((a, b) => b.created_at.getTime() - a.created_at.getTime());
      return { rows, rowCount: rows.length };
    }
    if (text.includes("JOIN foundry.persona")) {
      const sids = values[0] as string[];
      const rows = state.personas.filter((p) => sids.includes(p.sid));
      return { rows, rowCount: rows.length };
    }
    if (text.includes("FROM foundry.scene_changelog c")) {
      const [sceneId, actions] = values as [string, string[]];
      const rows = [
        ...state.changelog
          .filter((c) => c.scene_id === sceneId)
          .map((c) => ({
            at: new Date(),
            sid: c.sid,
            action: "changelog",
            body: c.note,
            source_note: "",
            origin: "visitor",
          })),
        ...state.actions
          .filter((a) => a.subject === sceneId && actions.includes(a.action))
          .map((a) => ({
            at: new Date(),
            sid: a.sid,
            action: a.action,
            body: "",
            source_note: "",
            origin: "visitor",
          })),
      ];
      return { rows, rowCount: rows.length };
    }
    if (text.includes("INSERT INTO foundry.scene_changelog")) {
      const [scene_id, note, sid] = values as [string, string, string];
      state.changelog.push({ scene_id, note, sid });
      return { rows: [], rowCount: 1 };
    }
    if (text.includes("INSERT INTO foundry.action_log")) {
      const [sid, action, subject] = values as [string, string, string];
      state.actions.push({ sid, action, subject });
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

const SCENE = "flagtag";

beforeEach(() => {
  state.stewards = [];
  state.transfers = [];
  state.actions = [];
  state.changelog = [];
  state.personas = [];
  state.nextTransferId = 1;
});

describe("claimSteward", () => {
  it("records a claim and logs it", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "I built it" });
    const { active, past, isViewerSteward } = await listStewards(SCENE, "sid-a");
    expect(active).toHaveLength(1);
    expect(past).toHaveLength(0);
    expect(isViewerSteward).toBe(true);
    expect(active[0].basis).toBe("I built it");
    expect(state.actions).toEqual([
      { sid: "sid-a", action: "claim_steward", subject: SCENE },
    ]);
  });

  it("refuses a double claim with the seat's own message", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await expect(
      claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" }),
    ).rejects.toThrow("You already steward this scene.");
  });

  it("an empty basis is accepted — the seat, not the statement, is the claim", async () => {
    await expect(
      claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" }),
    ).resolves.toBeUndefined();
  });

  it("never returns a raw sid, to the viewer or anyone else", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    const { active, isViewerSteward } = await listStewards(SCENE, "sid-b");
    expect(isViewerSteward).toBe(false);
    for (const row of active) {
      expect(row).not.toHaveProperty("sid");
    }
    expect(JSON.stringify(await listStewards(SCENE, "sid-a"))).not.toContain("sid-a");
  });
});

describe("releaseSteward", () => {
  it("refuses a release with no active seat", async () => {
    await expect(releaseSteward({ sceneId: SCENE, sid: "sid-a" })).rejects.toThrow(
      "You do not currently steward this scene.",
    );
  });

  it("closes the seat with release_reason 'self'", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await releaseSteward({ sceneId: SCENE, sid: "sid-a" });
    const { active, past, isViewerSteward } = await listStewards(SCENE, "sid-a");
    expect(active).toHaveLength(0);
    expect(past).toHaveLength(1);
    expect(past[0].releaseReason).toBe("self");
    expect(isViewerSteward).toBe(false);
  });
});

describe("steward-only writes", () => {
  it("refuses a note from a non-steward", async () => {
    await expect(
      addSceneNote({ sceneId: SCENE, sid: "sid-a", note: "hello" }),
    ).rejects.toThrow("Only an active steward can leave a note here.");
  });

  it("refuses an empty note before touching the seat check", async () => {
    await expect(
      addSceneNote({ sceneId: SCENE, sid: "sid-a", note: "   " }),
    ).rejects.toThrow("Write the note first.");
  });

  it("refuses a transfer offer from a non-steward", async () => {
    await expect(
      offerTransfer({ sceneId: SCENE, sid: "sid-a", note: "" }),
    ).rejects.toThrow("Only an active steward can offer a transfer.");
  });

  it("a steward's note lands in the changelog and the action log", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await addSceneNote({ sceneId: SCENE, sid: "sid-a", note: "swapped the spawn" });
    expect(state.changelog).toEqual([
      { scene_id: SCENE, note: "swapped the spawn", sid: "sid-a" },
    ]);
    expect(state.actions.map((a) => a.action)).toContain("scene_note");
  });

  it("a note renders once in the scene memory despite the dual write", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await addSceneNote({ sceneId: SCENE, sid: "sid-a", note: "swapped the spawn" });
    const memory = await listSceneMemory(SCENE);
    const noteRows = memory.filter((m) => m.body === "swapped the spawn");
    expect(noteRows).toHaveLength(1);
    expect(noteRows[0].action).toBe("changelog");
  });
});

describe("persona resolution", () => {
  it("actors render as the claimed persona name, else the badge", async () => {
    state.personas.push({ sid: "sid-a", display_name: "Ada" });
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await claimSteward({ sceneId: SCENE, sid: "sid-b", basis: "" });
    const { active } = await listStewards(SCENE);
    expect(active.map((s) => s.actor)).toEqual([
      { name: "Ada" },
      { badge: expect.any(String) },
    ]);

    await offerTransfer({ sceneId: SCENE, sid: "sid-a", note: "" });
    const [transfer] = await listTransfers(SCENE);
    expect(transfer.from).toEqual({ name: "Ada" });

    const memory = await listSceneMemory(SCENE);
    const claims = memory.filter((m) => m.action === "claim_steward");
    expect(claims.map((m) => m.actor)).toContainEqual({ name: "Ada" });
    expect(JSON.stringify({ active, transfer, memory })).not.toContain("sid-a");
  });
});

describe("revokeTransfer", () => {
  it("revokes only the offerer's own offered row", async () => {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "" });
    await offerTransfer({ sceneId: SCENE, sid: "sid-a", note: "" });
    const [row] = await listTransfers(SCENE);
    await expect(
      revokeTransfer({ transferId: row.id, sid: "sid-b" }),
    ).rejects.toThrow("That transfer offer is not open, or is not yours.");
    await revokeTransfer({ transferId: row.id, sid: "sid-a" });
    expect((await listTransfers(SCENE))[0].effectiveStatus).toBe("revoked");
  });
});

describe("acceptTransfer", () => {
  async function offeredCode(): Promise<string> {
    await claimSteward({ sceneId: SCENE, sid: "sid-a", basis: "made it" });
    const { code } = await offerTransfer({ sceneId: SCENE, sid: "sid-a", note: "yours" });
    return code;
  }

  it("flips the offer, closes the offerer's seat and opens the successor's", async () => {
    const code = await offeredCode();
    const res = await acceptTransfer({ code, sid: "sid-b" });
    expect(res.sceneId).toBe(SCENE);

    const transfer = state.transfers[0];
    expect(transfer.status).toBe("accepted");
    expect(transfer.accepted_sid).toBe("sid-b");

    const closed = state.stewards.find((s) => s.sid === "sid-a")!;
    expect(closed.released_at).not.toBeNull();
    expect(closed.release_reason).toBe("transfer");

    const opened = state.stewards.find((s) => s.sid === "sid-b")!;
    expect(opened.released_at).toBeNull();
    expect(opened.via_transfer_id).toBe(transfer.id);

    const { active } = await listStewards(SCENE, "sid-b");
    expect(active).toHaveLength(1);
    expect(active[0].viaTransfer).toBe(true);
  });

  it("refuses an unknown link", async () => {
    await expect(acceptTransfer({ code: "nope", sid: "sid-b" })).rejects.toThrow(
      "That transfer link is not valid.",
    );
  });

  it("refuses an already-accepted offer", async () => {
    const code = await offeredCode();
    await acceptTransfer({ code, sid: "sid-b" });
    await expect(acceptTransfer({ code, sid: "sid-c" })).rejects.toThrow(
      "That transfer was already accepted.",
    );
  });

  it("refuses a revoked offer", async () => {
    const code = await offeredCode();
    await revokeTransfer({ transferId: state.transfers[0].id, sid: "sid-a" });
    await expect(acceptTransfer({ code, sid: "sid-b" })).rejects.toThrow(
      "That transfer offer was revoked.",
    );
  });

  it("refuses an expired offer", async () => {
    const code = await offeredCode();
    state.transfers[0].expires_at = new Date(Date.now() - 1000);
    await expect(acceptTransfer({ code, sid: "sid-b" })).rejects.toThrow(
      "That transfer offer has expired.",
    );
  });

  it("every refusal above is a FoundryStateError, not a bare throw", async () => {
    await expect(acceptTransfer({ code: "nope", sid: "sid-b" })).rejects.toBeInstanceOf(
      FoundryStateError,
    );
  });
});
