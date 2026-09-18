import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import { MemoryRouter } from "react-router";

import DclTopBar from "@ui/web/frames/DclTopBar";
import { useChromeNav } from "@ui/web/frames/chrome-nav";
import ChromeNavBridge, { chromeHrefTarget } from "./ChromeNavBridge";

const ORIGIN = "https://catalyst.example.com";

describe("chromeHrefTarget", () => {
  it("routes the top bar's app pages in-app", () => {
    for (const href of ["/", "/shop", "/discover", "/create", "/blog", "/governance", "/marketplace/account", "/marketplace/packs"]) {
      expect(chromeHrefTarget(href, ORIGIN)).toEqual({ kind: "app", to: href });
    }
    expect(chromeHrefTarget("/shop?tab=all-assets#top", ORIGIN)).toEqual({ kind: "app", to: "/shop?tab=all-assets#top" });
  });

  it("leaves nginx-served surfaces and other origins to a document load", () => {
    expect(chromeHrefTarget("/docs/", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("/docs/creator/sdk7/", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("/play/", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("/play/?position=0,0", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("https://dcl.gg/discord", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("//evil.example/shop", ORIGIN)).toEqual({ kind: "document" });
    expect(chromeHrefTarget("/discover", ORIGIN).kind).toBe("app");
  });
});

function Probe() {
  return <span>{useChromeNav().navigate ? "bridged" : "plain"}</span>;
}

describe("ChromeNavBridge", () => {
  it("provides the chrome navigator so the top bar renders discoverable in-app links", () => {
    const html = renderToString(
      <MemoryRouter initialEntries={["/blog"]}>
        <ChromeNavBridge>
          <Probe />
          <DclTopBar variant="sites" active="learn" signedIn={false} />
        </ChromeNavBridge>
      </MemoryRouter>,
    );
    expect(html).toContain("bridged");
    expect(html).toMatch(/<a[^>]*href="\/shop"[^>]*data-discover="true"/);
    expect(html).toMatch(/<a[^>]*data-discover="true"[^>]*href="\/shop"|<a[^>]*href="\/shop"[^>]*data-discover="true"/);
  });

  it("renders plain anchors without the bridge", () => {
    const html = renderToString(<DclTopBar variant="sites" active="shop" signedIn={false} />);
    expect(html).not.toContain("data-discover");
    expect(html).toContain('href="/shop"');
  });
});
