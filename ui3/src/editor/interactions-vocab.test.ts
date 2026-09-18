import { describe, expect, it } from "vitest";
import { ACTION_CHIP, TRIGGER_CHIP } from "./interactions-vocab";
import { ACTIONS, TRIGGERS } from "./components/DeInteractionsPanel";

describe("interactions vocabulary", () => {
  it("every composer trigger and action has a chip phrase", () => {
    for (const t of TRIGGERS) {
      expect(TRIGGER_CHIP[t.id], `trigger ${t.id} has no chip phrase`).toBeTruthy();
    }
    for (const a of ACTIONS) {
      expect(ACTION_CHIP[a.id], `action ${a.id} has no chip phrase`).toBeTruthy();
    }
  });
});
