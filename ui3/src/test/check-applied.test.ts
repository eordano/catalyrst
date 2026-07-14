import { afterEach, beforeEach, expect, test } from "vitest";

import {
  resetValidationFailures,
  setValidationDevMode,
  validationFailures,
} from "../validate";
import { applyBridgePushForTest } from "../overlay/bridge";

// Spot-checks that the schema is APPLIED, not merely defined. The schemas have
// good tests; nothing tested that anything calls them, and the reviewer deleted
// all ten check() call sites with both suites green. These drive a bad payload
// through the real function and assert the boundary counted a failure, so
// removing the check is what fails the test.
//
// Production mode on purpose: the dev throw would be caught by the callers'
// own try/catch in some paths, and the counter is the signal that survives both.

beforeEach(() => {
  resetValidationFailures();
  setValidationDevMode(false);
});
afterEach(() => resetValidationFailures());

test("bridge/push validates what the engine sends", () => {
  applyBridgePushForTest({ kind: "identity", address: 42 });
  expect(validationFailures().get("bridge/push")).toBe(1);
});
