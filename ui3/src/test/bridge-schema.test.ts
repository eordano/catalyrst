import { describe, expect, test } from "vitest";
import { OverlayPushSchema } from "../generated/bridge-schemas";

// Every case below passed the guard that shipped before the schema landed
// (`typeof push.kind === "string"`), and the reducer's `?? prev.x` reads then
// kept the previous value rather than reporting anything. That combination is
// why engine-side drift used to surface as a stale UI rather than an error, so
// each case asserts BOTH that the schema now rejects it and that the old guard
// did not -- a case the old guard already caught would prove nothing.

const oldGuard = (v: unknown) =>
  typeof v === "object" && v !== null && typeof (v as { kind?: unknown }).kind === "string";

describe("bridge push validation", () => {
  const cases: [string, unknown, boolean][] = [
    ["valid identity", { kind: "identity", address: "0x1", signerAddress: "0x2", isGuest: false }, true],
    ["renamed field", { kind: "identity", addr: "0x1", signerAddress: "0x2", isGuest: false }, false],
    ["wrong type", { kind: "identity", address: 42, signerAddress: "0x2", isGuest: false }, false],
    ["unknown kind", { kind: "totallyNew", x: 1 }, false],
    ["chat missing timestamp", { kind: "chat", senderName: "a", senderAddress: "0x1", message: "hi", channel: "n" }, false],
  ];
  for (const [name, value, shouldPass] of cases) {
    test(name, () => {
      expect(OverlayPushSchema.safeParse(value).success).toBe(shouldPass);
      // Every one of these got past the guard that shipped before.
      expect(oldGuard(value)).toBe(true);
    });
  }
});
