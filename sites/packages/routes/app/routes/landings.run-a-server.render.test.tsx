import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";

import LdRunServerPage from "@ui/landings/pages/LdRunServerPage";

describe("LdRunServerPage SSR", () => {
  it("renders the three shapes, the steps, and both CTAs to the wizard", () => {
    const html = renderToString(
      <LdRunServerPage setupHref="/server/setup" serverHref="/server" />,
    ).replace(/<!-- -->/g, "");
    expect(html).toContain("Run your own server");
    expect(html).toContain("Content node");
    expect(html).toContain("Full realm");
    expect(html).toContain("Public gateway");
    expect(html).toContain("From zero to serving");
    expect(html.match(/href="\/server\/setup"/g)?.length).toBe(2);
    expect(html).toContain('href="/server"');
    expect(html).toContain("nixos-rebuild switch");
    expect(html).toContain("archive-capable Ethereum and Polygon RPC");
  });

  it("stays domain-neutral", () => {
    const html = renderToString(
      <LdRunServerPage setupHref="/server/setup" serverHref="/server" />,
    );
    expect(html).not.toMatch(/dcl\.one|decentraland\.org|interconnected/i);
  });
});
