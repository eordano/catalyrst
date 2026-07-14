import { describe, expect, it } from "vitest";

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import FdDeckPage from "@ui/foundry/pages/FdDeckPage";

// The deck reference page: the passages the site cites, verbatim from the
// archived capture, framed as advisor input — and nothing else from the deck.

describe("the deck reference page", () => {
  const html = renderToStaticMarkup(createElement(FdDeckPage));

  it("anchors exactly the five cited slides", () => {
    expect(html).toContain('id="slide-09"');
    expect(html).toContain('id="slide-10"');
    expect(html).toContain('id="slide-12"');
    expect(html).toContain('id="slide-13"');
    expect(html).toContain('id="slide-15"');
  });

  it("quotes the deck verbatim, archived typos included", () => {
    expect(html).toContain("6–24 active players + spectators");
    expect(html).toContain("8–50 recurring participants");
    expect(html).toContain("2–12 contributors + observers");
    expect(html).toContain("Design and instrument all three jobs");
    expect(html).toContain("Followers alone do not count");
    expect(html).toContain("draft emotional-job briefs and falsifiers");
    expect(html).toContain("Creator owns premise, values and intended human behavior");
    expect(html).toContain("playable demand");
    expect(html).toContain("Session Fill");
    expect(html).toContain("effectivness");
    expect(html).toContain("[sic]");
  });

  it("frames the deck as advisor input, not adopted strategy", () => {
    expect(html).toContain("advisor input, not adopted strategy");
    expect(html).toContain("source typos are preserved");
    expect(html).toContain("Nothing else from the deck is reproduced here.");
  });
});
