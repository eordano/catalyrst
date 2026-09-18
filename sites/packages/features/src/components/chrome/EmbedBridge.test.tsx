import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import { MemoryRouter } from "react-router";

import { EMBED_BOOT_SCRIPT, embedEscapeHref, isEmbedded } from "@ui/web/frames/embed";
import EmbedBridge from "./EmbedBridge";

describe("EmbedBridge", () => {
  it("renders nothing and stays inert without a document", () => {
    expect(isEmbedded()).toBe(false);
    const html = renderToString(
      <MemoryRouter initialEntries={["/play/?position=1,2"]}>
        <EmbedBridge />
      </MemoryRouter>,
    );
    expect(html).toBe("");
  });

  it("escapes the frame for pages outside the marketplace and keeps the rest", () => {
    expect(embedEscapeHref("/shop", "?tab=cart", "")).toBeNull();
    expect(embedEscapeHref("/marketplace/names", "", "")).toBeNull();
    expect(embedEscapeHref("/discover", "", "#events")).toBe("/discover#events");
    expect(embedEscapeHref("/play/", "?position=1,2", "")).toBe("/play/?position=1,2");
  });

  it("ships a boot script that only marks framed documents", () => {
    const attrs: Record<string, string> = {};
    const doc = { documentElement: { setAttribute: (k: string, v: string) => { attrs[k] = v; } } };
    const run = (self: object, top: object) => new Function("self", "top", "document", EMBED_BOOT_SCRIPT)(self, top, doc);
    const w = {};
    run(w, w);
    expect(attrs).toEqual({});
    run(w, {});
    expect(attrs).toEqual({ "data-embed": "1" });
  });
});
