import { describe, expect, it } from "vitest";

import { highlightsToList } from "./project-update-detail";

describe("highlightsToList", () => {
  it("strips bold, code, and markdown link syntax from bullet lines", () => {
    const md = [
      "- Shipped the **new** dashboard",
      "- See `README.md` for setup",
      "- Announced on [the forum](https://forum.example/t/1)",
    ].join("\n");

    expect(highlightsToList(md)).toEqual([
      "Shipped the new dashboard",
      "See README.md for setup",
      "Announced on the forum",
    ]);
  });
});
