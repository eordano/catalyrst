import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import ChromeLink from "./ChromeLink";
import { ChromeNavContext } from "./chrome-nav";
import CreatorHubChrome from "../../creatorhub/frames/CreatorHubChrome";
import NewShopTabs from "../../marketplace/new-shop/NewShopTabs";

afterEach(cleanup);

it("preserves an explicit navigation override when a router link is provided", () => {
  const navigate = vi.fn(() => true);
  const override = vi.fn(() => true);
  render(<ChromeNavContext value={{ navigate, Link: props => <a {...props} data-router-link /> }}>
    <ChromeLink href="/create" onNavigate={override}>Custom navigator</ChromeLink>
  </ChromeNavContext>);
  expect(fireEvent.click(screen.getByText("Custom navigator"))).toBe(false);
  expect(override).toHaveBeenCalledWith("/create");
  expect(navigate).not.toHaveBeenCalled();
});

it("routes creator navigation and shop link tabs through the existing app navigator", () => {
  const navigate = vi.fn(() => true);
  render(<ChromeNavContext value={{ navigate }}>
    <CreatorHubChrome active="home" />
    <NewShopTabs tabs={[{ id: "cart", label: "Cart", href: "/marketplace/cart" }]} />
  </ChromeNavContext>);
  expect(fireEvent.click(screen.getByRole("link", { name: "Scenes" }))).toBe(false);
  expect(navigate).toHaveBeenLastCalledWith("/create/scenes");
  expect(fireEvent.click(screen.getByText("Cart"))).toBe(false);
  expect(navigate).toHaveBeenLastCalledWith("/marketplace/cart");
});

it("preserves custom actions, modified clicks, downloads, fragments and external targets", () => {
  const navigate = vi.fn(() => true);
  const blocked = vi.fn(event => event.preventDefault());
  render(<ChromeNavContext value={{ navigate }}>
    <ChromeLink href="/create" onClick={blocked}>Custom</ChromeLink>
    <ChromeLink href="/create">Ordinary</ChromeLink>
    <ChromeLink href="/create" download>Download</ChromeLink>
    <ChromeLink href="/create" target="_blank">New tab</ChromeLink>
    <ChromeLink href="#main">Skip</ChromeLink>
  </ChromeNavContext>);
  fireEvent.click(screen.getByText("Custom"));
  fireEvent.click(screen.getByText("Ordinary"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Download"));
  fireEvent.click(screen.getByText("New tab"));
  fireEvent.click(screen.getByText("Skip"));
  expect(blocked).toHaveBeenCalledOnce();
  expect(navigate).not.toHaveBeenCalled();
});
