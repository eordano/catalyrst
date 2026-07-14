import { afterEach, beforeEach, expect, test } from "vitest";
import { z } from "zod";
import { check, resetValidationFailures, setValidationDevMode, setValidationReporter } from "../validate";

// The P0 this pins: several persisted stores are keyed by the signer address, so
// the record KEY lands in the zod issue path -- and `detail` reaches a
// console.warn that sites patches globally and POSTs to /internal/client-error.
// "Paths, never values" was not enough on its own.
const Store = z.record(z.string(), z.object({ txHash: z.string() }));
const WALLET = "0x1d9fd6a04e5e1cbb0f5b3ac7a0d0dbd8c0d63e11";

beforeEach(() => { resetValidationFailures(); setValidationDevMode(false); });
afterEach(() => resetValidationFailures());

test("a wallet-keyed path is redacted before it can reach any sink", () => {
  const seen: { detail: string; paths: string[] }[] = [];
  setValidationReporter((r) => seen.push(r));
  check(Store, { [WALLET]: { txHash: 42 } }, "t/pending");
  const blob = JSON.stringify(seen);
  expect(blob).not.toContain(WALLET);
  expect(blob).not.toContain("0x1d9fd6");
  // The shape must survive, or the report stops being actionable.
  expect(seen[0]?.paths).toEqual(["<key>.txHash"]);
});

test("the reporter fires once per boundary, not once per rejection", () => {
  let calls = 0;
  setValidationReporter(() => { calls += 1; });
  for (let i = 0; i < 10; i++) check(Store, { [WALLET]: { txHash: i } }, "t/flood");
  expect(calls).toBe(1);
});
