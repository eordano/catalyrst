import { createHash } from "node:crypto";

import { beforeEach, describe, expect, it, vi } from "vitest";

import asks from "../../fixtures/foundry-asks.json";
import { askHandleVariants, type ImportedAsk } from "./exchange.server";
import {
  isHandleReserved,
  mintCarryCode,
  normalizeCarryCode,
  redeemCarryCode,
  validateDisplayName,
} from "./persona.server";

// Enough of Postgres to exercise both carry-code paths in memory. The stub is
// a dispatcher keyed on the SQL each function actually sends; the assertions
// cover the contract — plaintext never stored, single-use burn recorded,
// supersession revokes — not SQL syntax.
type CodeRow = {
  id: number;
  sid: string;
  code_hash: string;
  revoked_at: string | null;
  redeemed_at: string | null;
  redeemed_from: string | null;
};

const carryState = vi.hoisted(() => ({
  personas: new Map<string, string>(),
  codes: [] as {
    id: number;
    sid: string;
    code_hash: string;
    revoked_at: string | null;
    redeemed_at: string | null;
    redeemed_from: string | null;
  }[],
  aliases: [] as { alias_sid: string; persona_sid: string }[],
  actions: [] as { sid: string; action: string; detail: string }[],
  nextId: 1,
}));

vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  const s = carryState;
  async function query(sql: string, values: unknown[] = []) {
    if (/SELECT 1 FROM foundry\.persona/.test(sql)) {
      const has = s.personas.has(values[0] as string);
      return { rowCount: has ? 1 : 0, rows: [] };
    }
    if (/SELECT display_name FROM foundry\.persona/.test(sql)) {
      const name = s.personas.get(values[0] as string);
      return name
        ? { rowCount: 1, rows: [{ display_name: name }] }
        : { rowCount: 0, rows: [] };
    }
    if (/UPDATE foundry\.persona_carry_code SET revoked_at/.test(sql)) {
      const active = s.codes.filter(
        (c) => c.sid === values[0] && c.revoked_at === null && c.redeemed_at === null,
      );
      for (const c of active) c.revoked_at = "now";
      return { rowCount: active.length, rows: [] };
    }
    if (/INSERT INTO foundry\.persona_carry_code/.test(sql)) {
      s.codes.push({
        id: s.nextId++,
        sid: values[0] as string,
        code_hash: values[1] as string,
        revoked_at: null,
        redeemed_at: null,
        redeemed_from: null,
      });
      return { rowCount: 1, rows: [] };
    }
    if (/FROM foundry\.persona_carry_code\s+WHERE code_hash/.test(sql)) {
      const row = s.codes.find((c) => c.code_hash === values[0]);
      return row ? { rowCount: 1, rows: [row] } : { rowCount: 0, rows: [] };
    }
    if (/UPDATE foundry\.persona_carry_code\s+SET redeemed_at/.test(sql)) {
      const row = s.codes.find((c) => c.id === values[0]);
      if (row) {
        row.redeemed_at = "now";
        row.redeemed_from = values[1] as string;
      }
      return { rowCount: row ? 1 : 0, rows: [] };
    }
    if (/INSERT INTO foundry\.sid_alias/.test(sql)) {
      const exists = s.aliases.some((a) => a.alias_sid === values[0]);
      if (!exists) {
        s.aliases.push({
          alias_sid: values[0] as string,
          persona_sid: values[1] as string,
        });
      }
      return { rowCount: exists ? 0 : 1, rows: [] };
    }
    if (/INSERT INTO foundry\.action_log/.test(sql)) {
      s.actions.push({
        sid: values[0] as string,
        action: values[1] as string,
        detail: values[3] as string,
      });
      return { rowCount: 1, rows: [] };
    }
    throw new Error(`unexpected SQL: ${sql}`);
  }
  const client = { query };
  return {
    ...actual,
    assertRate: () => {},
    getPool: () => ({ query }),
    withTx: async (fn: (c: unknown) => Promise<unknown>) => fn(client),
  };
});

function sha256(code: string): string {
  return createHash("sha256").update(normalizeCarryCode(code)).digest("hex");
}

function activeCodes(sid: string): CodeRow[] {
  return carryState.codes.filter(
    (c) => c.sid === sid && c.revoked_at === null && c.redeemed_at === null,
  );
}

// Mirrors the persona_name_* CHECKs in schema.sql: the route rejects with the
// form's own sentence instead of surfacing a constraint violation.
describe("validateDisplayName", () => {
  it("rejects characters outside the DB charset", () => {
    expect(validateDisplayName("zap!")).toBe(
      "Letters, numbers, spaces and . _ - only.",
    );
  });

  it("rejects names outside 2–32 characters", () => {
    expect(validateDisplayName("z")).toBe(
      "2–32 characters. Letters, numbers, spaces and . _ - only.",
    );
    expect(validateDisplayName("z".repeat(33))).toBe(
      "2–32 characters. Letters, numbers, spaces and . _ - only.",
    );
  });

  it("rejects the reserved names, case-insensitively", () => {
    expect(validateDisplayName("admin")).toBe("That name is reserved.");
    expect(validateDisplayName("Visitor")).toBe("That name is reserved.");
  });

  it("accepts a name the DB would accept", () => {
    expect(validateDisplayName("Zap Rowsdower._-1")).toBeNull();
  });
});

// The import reserves each quoted author's handle so a visitor cannot claim it
// and stand next to that author's words. The collision rule is the same
// case-insensitive comparison the reserved_handle_ci index enforces, checked
// here against the real fixture the importer lands.
describe("imported-ask handles are reserved for claim", () => {
  const handles = (asks as ImportedAsk[]).flatMap((a) => askHandleVariants(a.author));

  it("rejects a fixture author's handle, case-insensitively", () => {
    expect(handles).toContain("JeyJey64");
    expect(isHandleReserved("jeyjey64", handles)).toBe(true);
    expect(isHandleReserved("JEYJEY64", handles)).toBe(true);
  });

  it("reserves the bare name behind a reddit u/ prefix too", () => {
    expect(handles).toContain("u/degeneratehodl");
    expect(isHandleReserved("degeneratehodl", handles)).toBe(true);
  });

  it("leaves unrelated names claimable", () => {
    expect(isHandleReserved("Zap Rowsdower", handles)).toBe(false);
  });
});

describe("carry codes", () => {
  beforeEach(() => {
    carryState.personas.clear();
    carryState.codes.length = 0;
    carryState.aliases.length = 0;
    carryState.actions.length = 0;
    carryState.nextId = 1;
  });

  it("normalizes presentation: dashes and case never change the code", () => {
    expect(normalizeCarryCode("AbCdE-fghjk-MNPQR-23456")).toBe(
      "abcdefghjkmnpqr23456",
    );
  });

  it("mints a grouped 20-char code and stores only its sha256", async () => {
    carryState.personas.set("sid-a", "Zap");
    const minted = await mintCarryCode("sid-a");
    expect(minted.code).toMatch(
      /^[a-hj-km-np-z2-9]{5}-[a-hj-km-np-z2-9]{5}-[a-hj-km-np-z2-9]{5}-[a-hj-km-np-z2-9]{5}$/,
    );
    expect(minted.replaced).toBe(false);
    expect(carryState.codes).toHaveLength(1);
    expect(carryState.codes[0].code_hash).toBe(sha256(minted.code));
    expect(carryState.codes[0].code_hash).not.toContain(
      normalizeCarryCode(minted.code),
    );
    const act = carryState.actions.find((a) => a.action === "mint_carry_code");
    expect(act?.sid).toBe("sid-a");
    expect(act?.detail).toBe("{}");
  });

  it("a second mint supersedes the first, which then refuses to redeem", async () => {
    carryState.personas.set("sid-a", "Zap");
    const first = await mintCarryCode("sid-a");
    const second = await mintCarryCode("sid-a");
    expect(second.replaced).toBe(true);
    expect(activeCodes("sid-a")).toHaveLength(1);
    await expect(redeemCarryCode("sid-b", first.code)).rejects.toThrow(
      /single-use/,
    );
  });

  it("refuses to mint without a claimed persona", async () => {
    await expect(mintCarryCode("sid-nobody")).rejects.toThrow(
      /Claim a persona first/,
    );
  });

  it("redeems into the persona's own sid, burning the code once", async () => {
    carryState.personas.set("sid-a", "Zap");
    const minted = await mintCarryCode("sid-a");
    const redeemed = await redeemCarryCode("sid-fresh", minted.code);
    expect(redeemed).toEqual({ sid: "sid-a", displayName: "Zap" });
    const row = carryState.codes[0];
    expect(row.redeemed_at).not.toBeNull();
    expect(row.redeemed_from).toBe("sid-fresh");
    expect(
      carryState.actions.some(
        (a) => a.action === "redeem_carry_code" && a.sid === "sid-a",
      ),
    ).toBe(true);
    await expect(redeemCarryCode("sid-other", minted.code)).rejects.toThrow(
      /single-use/,
    );
  });

  it("aliases the abandoned fresh sid onto the persona", async () => {
    carryState.personas.set("sid-a", "Zap");
    const minted = await mintCarryCode("sid-a");
    await redeemCarryCode("sid-fresh", minted.code);
    expect(carryState.aliases).toEqual([
      { alias_sid: "sid-fresh", persona_sid: "sid-a" },
    ]);
  });

  it("accepts a hand-mangled but equivalent code", async () => {
    carryState.personas.set("sid-a", "Zap");
    const minted = await mintCarryCode("sid-a");
    const mangled = minted.code.toUpperCase().replace(/-/g, " ");
    const redeemed = await redeemCarryCode("sid-fresh", mangled);
    expect(redeemed.sid).toBe("sid-a");
  });

  it("never aliases a sid that owns a persona of its own", async () => {
    carryState.personas.set("sid-a", "Zap");
    carryState.personas.set("sid-b", "Rowsdower");
    const minted = await mintCarryCode("sid-a");
    const redeemed = await redeemCarryCode("sid-b", minted.code);
    // The cookie re-issue proceeds — Rowsdower stays reachable through its own
    // return code — but Rowsdower's acts are never folded into Zap's.
    expect(redeemed.sid).toBe("sid-a");
    expect(carryState.codes[0].redeemed_at).not.toBeNull();
    expect(carryState.aliases).toEqual([]);
  });

  it("refuses a same-session redeem without burning the code", async () => {
    carryState.personas.set("sid-a", "Zap");
    const minted = await mintCarryCode("sid-a");
    await expect(redeemCarryCode("sid-a", minted.code)).rejects.toThrow(
      /already holds that persona/,
    );
    expect(carryState.codes[0].redeemed_at).toBeNull();
  });

  it("refuses an unknown or malformed code with the same sentence", async () => {
    await expect(redeemCarryCode("sid-x", "short")).rejects.toThrow(
      /doesn't open anything/,
    );
    await expect(
      redeemCarryCode("sid-x", "aaaaa-bbbbb-ccccc-ddddd"),
    ).rejects.toThrow(/doesn't open anything/);
  });
});
