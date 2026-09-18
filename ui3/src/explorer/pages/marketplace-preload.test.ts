import { expect, it } from "vitest";
import { warmMarketplace } from "./marketplace-preload";

it("retains the loaded marketplace document and replaces it when the viewer changes", () => {
  const first = warmMarketplace("alice", "about:blank");
  expect(first.parking.hidden).toBe(true);
  expect(first.frame.hasAttribute("credentialless")).toBe(true);
  first.frame.dispatchEvent(new Event("load"));
  expect(warmMarketplace("alice", "about:blank")).toBe(first);
  expect(first.loaded).toBe(true);
  const next = warmMarketplace("bob", "about:blank");
  expect(next.frame).not.toBe(first.frame);
  expect(first.frame.isConnected).toBe(false);
  expect(next.frame.isConnected).toBe(true);
  next.parking.remove();
});
