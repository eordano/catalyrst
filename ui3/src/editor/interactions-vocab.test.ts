// The composer may support fewer ids than the vocabulary, never more: an id the
// composer emits without a chip phrase would reach the ribbon as a raw
// "on_click"-style string.
import { describe, expect, it } from "vitest";
import { ACTION_CHIP, TRIGGER_CHIP } from "./interactions-vocab";
import { ACTIONS, TRIGGERS } from "./components/DeInteractionsPanel";

describe("interactions vocabulary", () => {
  it("every composer trigger has a chip phrase", () => {
    for (const t of TRIGGERS) {
      expect(TRIGGER_CHIP[t.id], `trigger ${t.id} has no chip phrase`).toBeTruthy();
    }
  });
  it("every composer action has a chip phrase", () => {
    for (const a of ACTIONS) {
      expect(ACTION_CHIP[a.id], `action ${a.id} has no chip phrase`).toBeTruthy();
    }
  });
});
