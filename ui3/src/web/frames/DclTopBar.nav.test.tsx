import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import DclTopBar from "./DclTopBar";
import ChromeShell from "../../components/ChromeShell";
import { ChromeNavContext, chromeLinkProps, isSameDocumentHref, type ChromeNavigate } from "./chrome-nav";
import { ChromeAuthContext, type ChromeAuth } from "./chrome-auth";
import { getJSON } from "../../data/catalyst/client";

vi.mock("../../data/catalyst/client", async importOriginal => ({
  ...await importOriginal<typeof import("../../data/catalyst/client")>(),
  getJSON: vi.fn(async () => ({})),
}));

afterEach(() => cleanup());

function mountBar(navigate: ChromeNavigate | undefined) {
  return render(
    <ChromeNavContext.Provider value={{ navigate }}>
      <DclTopBar variant="sites" active="whatson" signedIn={false} />
    </ChromeNavContext.Provider>,
  );
}

describe("top bar links hand same-document hrefs to the chrome navigator", () => {
  test("reuses the shared profile on page entry and clears the avatar on an account switch", () => {
    vi.mocked(getJSON).mockClear();
    const auth: ChromeAuth = {
      signedIn: true, account: "0x1111", name: "Alice", mana: "", committee: false,
      avatarUrl: "https://images.example/alice.png",
    };
    const { rerender } = render(<ChromeAuthContext value={auth}><DclTopBar /></ChromeAuthContext>);
    expect(document.querySelector<HTMLImageElement>(".dtb__avatar img")?.src).toBe(auth.avatarUrl);
    rerender(<ChromeAuthContext value={{ ...auth, account: "0x2222", name: "", avatarUrl: undefined }}><DclTopBar /></ChromeAuthContext>);
    expect(document.querySelector(".dtb__avatar img")).toBeNull();
    expect(getJSON).not.toHaveBeenCalled();
  });

  test("a plain click on Shop is routed through the navigator and the document load is prevented", () => {
    const navigate = vi.fn<ChromeNavigate>(() => true);
    mountBar(navigate);
    const shop = screen.getAllByRole("link", { name: "Shop" })[0]!;
    expect(shop).toHaveAttribute("href", "/shop");
    expect(shop).toHaveAttribute("data-discover", "true");
    const notPrevented = fireEvent.click(shop);
    expect(navigate).toHaveBeenCalledWith("/shop");
    expect(notPrevented).toBe(false);
  });

  test("modified clicks keep the browser's own new-tab behaviour", () => {
    const navigate = vi.fn<ChromeNavigate>(() => true);
    mountBar(navigate);
    const shop = screen.getAllByRole("link", { name: "Shop" })[0]!;
    expect(fireEvent.click(shop, { ctrlKey: true })).toBe(true);
    expect(fireEvent.click(shop, { metaKey: true })).toBe(true);
    expect(fireEvent.click(shop, { button: 1 })).toBe(true);
    expect(navigate).not.toHaveBeenCalled();
  });

  test("when the navigator declines an href the anchor falls back to a document load", () => {
    const navigate = vi.fn<ChromeNavigate>((href) => href !== "/docs/");
    mountBar(navigate);
    const docs = document.querySelector<HTMLAnchorElement>(".dtb__dditem[href=\"/docs/\"]")!;
    expect(fireEvent.click(docs)).toBe(true);
    expect(navigate).toHaveBeenCalledWith("/docs/");
    const blog = document.querySelector<HTMLAnchorElement>(".dtb__dditem[href=\"/blog\"]")!;
    expect(fireEvent.click(blog)).toBe(false);
  });

  test("without a navigator the links are plain anchors", () => {
    mountBar(undefined);
    const shop = screen.getAllByRole("link", { name: "Shop" })[0]!;
    expect(shop).not.toHaveAttribute("data-discover");
    expect(fireEvent.click(shop)).toBe(true);
  });

  test("ChromeShell tabs read the same navigator", () => {
    const navigate = vi.fn<ChromeNavigate>(() => true);
    render(
      <ChromeNavContext.Provider value={{ navigate }}>
        <ChromeShell tabs={[{ id: "cart", label: "Cart", href: "/marketplace/cart" }]} active="cart" tabsLabel="Sections" />
      </ChromeNavContext.Provider>,
    );
    const cart = screen.getAllByRole("link", { name: "Cart" })[0]!;
    expect(fireEvent.click(cart)).toBe(false);
    expect(navigate).toHaveBeenCalledWith("/marketplace/cart");
  });
});

describe("chromeLinkProps", () => {
  test("only same-document hrefs are discoverable or intercepted", () => {
    expect(isSameDocumentHref("/shop")).toBe(true);
    expect(isSameDocumentHref("//evil.example/shop")).toBe(false);
    expect(isSameDocumentHref("https://catalyst.example.com/shop")).toBe(false);
    expect(isSameDocumentHref("mailto:x@y.z")).toBe(false);
    const go = vi.fn<ChromeNavigate>(() => true);
    expect(chromeLinkProps("https://x.example/", go)["data-discover"]).toBeUndefined();
    expect(chromeLinkProps("/shop", go)["data-discover"]).toBe("true");
    expect(chromeLinkProps("/shop", undefined)["data-discover"]).toBeUndefined();
  });
});
