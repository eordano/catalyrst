import { describe, expect, test } from "vitest";
import { sdkConnectionTransition } from "./SdkEditorWorkspace";

describe("SDK connection lifecycle", () => {
  test("rejects stale connect, read, and retry completion", () => {
    const connecting = sdkConnectionTransition({ phase: "connecting", url: "https://one.test", request: 0 }, { type: "start", url: "https://one.test", request: 1 });
    const replacement = sdkConnectionTransition(connecting, { type: "start", url: "https://two.test", request: 2 });
    expect(sdkConnectionTransition(replacement, { type: "error", url: "https://one.test", request: 1, error: "old" })).toEqual(replacement);
    expect(sdkConnectionTransition(replacement, { type: "error", url: "https://two.test", request: 2, error: "retry failed" }).phase).toBe("error");
  });
});
