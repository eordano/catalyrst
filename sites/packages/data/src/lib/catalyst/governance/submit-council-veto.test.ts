import { describe, expect, it } from "vitest";

import {
  councilSpaceId,
  parseDecisionUrl,
  validateDecisionUrl,
} from "./submit-council-veto";

const COUNCIL = "https://snapshot.org/#/dao-council.dcl.eth/proposal/0xabc123";
const MAIN_SPACE = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123";

describe("parseDecisionUrl \u{2014} council space is required", () => {
  it("reads the council space id out of the configured space URL and accepts a proposal in that space", () => {
    expect(councilSpaceId()).toBe("dao-council.dcl.eth");
    expect(councilSpaceId("https://snapshot.org/#/other.eth")).toBe("other.eth");

    expect(parseDecisionUrl(COUNCIL)).toMatchObject({
      snapshotId: "0xabc123",
      space: "dao-council.dcl.eth",
      valid: true,
    });
  });

  it("rejects a proposal from any other snapshot space and a non-snapshot host even when the path looks right", () => {
    const ref = parseDecisionUrl(MAIN_SPACE);
    expect(ref.space).toBe("snapshot.dcl.eth");
    expect(ref.valid).toBe(false);
    expect(validateDecisionUrl(MAIN_SPACE).decision_snapshot_id).toBeTruthy();

    expect(
      parseDecisionUrl("https://snapshot.example.com/#/dao-council.dcl.eth/proposal/0xabc123")
        .valid,
    ).toBe(false);
  });

  it("rejects a space landing page, a bare proposal id, and an empty or unparseable url", () => {
    expect(parseDecisionUrl("https://snapshot.org/#/dao-council.dcl.eth").valid).toBe(false);
    expect(parseDecisionUrl("https://snapshot.org/#/dao-council.dcl.eth").snapshotId).toBe("");
    expect(parseDecisionUrl("https://snapshot.org/#/proposal/0xabc123").valid).toBe(false);
    expect(parseDecisionUrl("").valid).toBe(false);
    expect(parseDecisionUrl("not a url").valid).toBe(false);
  });
});
