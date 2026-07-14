import { beforeEach, describe, expect, it, vi } from "vitest";

import { mintInvite } from "./roles.server";

const state = vi.hoisted(() => ({
  hostSids: [] as string[],
  stewardConsentSids: [] as string[],
  invites: [] as unknown[][],
  actions: [] as unknown[][],
}));

// Enough of Postgres to run mintInvite — requireHost included, real SQL against
// the fake dispatcher — in memory. db.server's errors stay real.
vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  async function query(
    text: string,
    values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    if (text.includes("FROM foundry.role_grant")) {
      const held = state.hostSids.includes(String(values[0]));
      return { rows: held ? [{ one: 1 }] : [], rowCount: held ? 1 : 0 };
    }
    if (text.includes("FROM foundry.consent_event")) {
      const granted = state.stewardConsentSids.includes(String(values[0]));
      return {
        rows: granted ? [{ state: "granted" }] : [],
        rowCount: granted ? 1 : 0,
      };
    }
    if (text.includes("INSERT INTO foundry.role_invite")) {
      state.invites.push(values);
      return { rows: [], rowCount: 1 };
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

const HOST = "sid-host";

beforeEach(() => {
  state.hostSids = [HOST];
  state.stewardConsentSids = [HOST];
  state.invites = [];
  state.actions = [];
});

describe("mintInvite", () => {
  it("a host mints a code that lands verbatim in role_invite, attributed", async () => {
    const { code } = await mintInvite({
      sid: HOST,
      role: "create",
      note: "for the tuesday playtest crew",
      expiresAt: null,
      ip: null,
    });
    expect(code).toMatch(/^[0-9a-f]{18}$/);
    expect(state.invites).toHaveLength(1);
    expect(state.invites[0]).toEqual([
      code,
      "create",
      "for the tuesday playtest crew",
      HOST,
      null,
    ]);
  });

  it("the recorded act carries who, role and note — never the code", async () => {
    const { code } = await mintInvite({
      sid: HOST,
      role: "host",
      note: "successor",
      expiresAt: null,
      ip: null,
    });
    expect(state.actions).toHaveLength(1);
    const [sid, action, subject, detail] = state.actions[0] as string[];
    expect(sid).toBe(HOST);
    expect(action).toBe("mint_invite");
    expect(subject).toBe("host");
    expect(JSON.parse(detail)).toEqual({ role: "host", note: "successor" });
    // The regression trap for the leak: action_log feeds the public timeline,
    // so no logged parameter may contain the live code.
    for (const value of state.actions[0] as unknown[]) {
      expect(String(value)).not.toContain(code);
    }
  });

  it("refuses without the host role, and with the consent withdrawn", async () => {
    await expect(
      mintInvite({ sid: "sid-plain", role: "start", note: "", expiresAt: null }),
    ).rejects.toThrow(/host role/);

    state.stewardConsentSids = [];
    await expect(
      mintInvite({ sid: HOST, role: "start", note: "", expiresAt: null }),
    ).rejects.toThrow(/steward-code/);
    expect(state.invites).toHaveLength(0);
    expect(state.actions).toHaveLength(0);
  });
});
