import { afterEach, beforeEach, expect, test } from "vitest";

import {
  resetValidationFailures,
  setValidationDevMode,
  validationFailures,
} from "../validate";
import { applyBridgePushForTest } from "../overlay/bridge";

beforeEach(() => {
  resetValidationFailures();
  setValidationDevMode(false);
});
afterEach(() => resetValidationFailures());

test("bridge/push validates what the engine sends", () => {
  applyBridgePushForTest({ kind: "identity", address: 42 });
  expect(validationFailures().get("bridge/push")).toBe(1);
});

test("bridge/push accepts a permission withdrawal the store has no state for", () => {
  applyBridgePushForTest({ kind: "permissionWithdrawn", id: 3 });
  expect(validationFailures().get("bridge/push")).toBeUndefined();
});
