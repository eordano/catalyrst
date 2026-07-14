import { describe, expect, it } from "vitest";

import { StoryParseError, parseStoryContent } from "./context";

const VALID = `---
id: places-hero
status: running
owner: eordano
hypothesis:
  statement: A larger hero lifts engagement
  because: it surfaces live places sooner
metric:
  primary: place_card_click
  guardrails:
    - bounce_rate
experiment:
  key: places-hero-size
  unit: session
  variants:
    - id: control
      weight: 0.5
      flags: { heroSize: small }
    - id: treatment
      weight: 0.5
      flags: { heroSize: large }
  baseline: 0.12
  mde: 0.02
  min_sample: 4000
decision:
  rule: ship if primary up and guardrails flat
---

# Body is ignored by the parser.
`;

describe("parseStoryContent", () => {
  it("parses + validates a well-formed story.md", () => {
    const meta = parseStoryContent(VALID, "valid.md");
    expect(meta.id).toBe("places-hero");
    expect(meta.experiment.key).toBe("places-hero-size");
    expect(meta.experiment.variants).toHaveLength(2);
    expect(meta.experiment.variants[0].flags).toEqual({ heroSize: "small" });
    expect(meta.metric.guardrails).toEqual(["bounce_rate"]);
  });

  it("defaults variant.flags to {} when omitted", () => {
    const noFlags = VALID.replace("      flags: { heroSize: small }\n", "");
    const meta = parseStoryContent(noFlags, "noflags.md");
    expect(meta.experiment.variants[0].flags).toEqual({});
  });

  it("throws StoryParseError when required fields are missing", () => {
    const broken = VALID.replace("  key: places-hero-size\n", "");
    expect(() => parseStoryContent(broken, "broken.md")).toThrow(StoryParseError);
  });

  it("throws when there are zero variants", () => {
    const noVariants = `---
id: x
status: draft
owner: o
hypothesis: { statement: a, because: b }
metric: { primary: m, guardrails: [] }
experiment:
  key: k
  unit: session
  variants: []
decision: { rule: r }
---
`;
    expect(() => parseStoryContent(noVariants, "novar.md")).toThrow(
      StoryParseError,
    );
  });
});
