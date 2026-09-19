import { beforeEach, describe, expect, it, vi } from "vitest";

import { loadShopScreen } from "@data/lib/screens/shop.server";
import { headers as documentHeaders, loader } from "./shop";
import { loader as resourceLoader } from "./api.screens.v1.shop";

vi.mock("@data/lib/screens/shop.server", () => ({ loadShopScreen: vi.fn() }));

beforeEach(() => {
  vi.mocked(loadShopScreen).mockReset().mockResolvedValue({
    data: {
      filters: { tab: "overview", page: 0, category: "", rarity: "", search: "", sortBy: "recently_listed" },
      cards: [], topCards: [], trendingCards: [], total: 0, fallback: false,
      sections: { catalog: "ready", top: "ready", trending: "ready" },
    },
    serverTiming: "shop_total;dur=12.3",
  });
});

describe("shop screen transports", () => {
  it("preserves the SSR session cookie and reports server timing without public caching", async () => {
    const request = new Request("https://sites.test/shop?tab=all-assets&page=2");
    const result = await loader({ request, params: {}, context: {} } as never);
    const headers = new Headers(result.init?.headers);
    expect(headers.get("Set-Cookie")).toContain("sid=");
    expect(headers.get("Server-Timing")).toBe("shop_total;dur=12.3");
    expect(headers.get("Cache-Control")).toBe("private, no-store");
    expect(result.data.sid).toBeTruthy();
    expect(loadShopScreen).toHaveBeenCalledWith(new URLSearchParams("tab=all-assets&page=2"), request.signal);
    const document = documentHeaders({
      loaderHeaders: headers,
      parentHeaders: new Headers({ "X-Parent": "preserved" }),
    } as never);
    expect(document.get("Server-Timing")).toBe("shop_total;dur=12.3");
    expect(document.get("Cache-Control")).toBe("private, no-store");
    expect(document.get("X-Parent")).toBe("preserved");
  });

  it("serves the versioned public JSON contract without a session id or cookie", async () => {
    const request = new Request("https://sites.test/api/screens/v1/shop");
    const response = await resourceLoader({ request });
    const body = await response.json();
    expect(body).toMatchObject({ version: 1, cards: [], sections: { catalog: "ready" } });
    expect(body).not.toHaveProperty("sid");
    expect(response.headers.has("Set-Cookie")).toBe(false);
    expect(response.headers.get("Server-Timing")).toBe("shop_total;dur=12.3");
    expect(response.headers.get("Cache-Control")).toBe("no-store");
  });
});
