import { describe, expect, it } from "vitest";

import {
  councilSpaceId,
  parseDecisionUrl,
  validateDecisionUrl,
} from "./submit-council-veto";

const COUNCIL = "https://snapshot.org/#/dao-council.dcl.eth/proposal/0xabc123";
const MAIN_SPACE = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123";

describe("parseDecisionUrl \u{2014} council space is required", () => {
  it("reads the council space id out of the configured space URL", () => {
    expect(councilSpaceId()).toBe("dao-council.dcl.eth");
    expect(councilSpaceId("https://snapshot.org/#/other.eth")).toBe("other.eth");
  });

  it("accepts a proposal in the council space", () => {
    const ref = parseDecisionUrl(COUNCIL);
    expect(ref).toMatchObject({
      snapshotId: "0xabc123",
      space: "dao-council.dcl.eth",
      valid: true,
    });
  });

  it("rejects a proposal from any other snapshot space", () => {
    const ref = parseDecisionUrl(MAIN_SPACE);
    expect(ref.space).toBe("snapshot.dcl.eth");
    expect(ref.valid).toBe(false);
    expect(validateDecisionUrl(MAIN_SPACE).decision_snapshot_id).toBeTruthy();
  });

  it("rejects a non-snapshot host even when the path looks right", () => {
    expect(
      parseDecisionUrl("https://snapshot.example.com/#/dao-council.dcl.eth/proposal/0xabc123")
        .valid,
    ).toBe(false);
  });

  it("rejects a space landing page with no proposal segment", () => {
    expect(parseDecisionUrl("https://snapshot.org/#/dao-council.dcl.eth").valid).toBe(false);
    expect(parseDecisionUrl("https://snapshot.org/#/dao-council.dcl.eth").snapshotId).toBe("");
  });

  it("rejects a bare proposal id with no space segment", () => {
    expect(parseDecisionUrl("https://snapshot.org/#/proposal/0xabc123").valid).toBe(false);
  });

  it("rejects an empty or unparseable url", () => {
    expect(parseDecisionUrl("").valid).toBe(false);
    expect(parseDecisionUrl("not a url").valid).toBe(false);
  });
});
