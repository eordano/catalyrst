import { afterEach, describe, expect, test, vi } from "vitest";
import { act, render } from "@testing-library/react";

import { EMBED_ESCAPE_MESSAGE } from "../../web/frames/embed";
import MarketplaceFrame from "./MarketplaceFrame";

function message(data: unknown, source: unknown) {
  const ev = new Event("message") as Event & { data?: unknown; source?: unknown };
  Object.defineProperty(ev, "data", { value: data });
  Object.defineProperty(ev, "source", { value: source });
  return ev;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("MarketplaceFrame escape relay", () => {
  test("an escape marker posted by the framed shop becomes an Escape keydown on the parent window, other messages do not", () => {
    const keys: string[] = [];
    const onKey = (e: KeyboardEvent) => keys.push(e.key);
    window.addEventListener("keydown", onKey, true);
    const { container, unmount } = render(<MarketplaceFrame src="about:blank" />);
    const frame = container.querySelector<HTMLIFrameElement>("iframe.mkf__frame")!;

    act(() => {
      window.dispatchEvent(message(EMBED_ESCAPE_MESSAGE, window));
      window.dispatchEvent(message("hello", frame.contentWindow));
    });
    expect(keys).toEqual([]);

    act(() => {
      window.dispatchEvent(message(EMBED_ESCAPE_MESSAGE, frame.contentWindow));
    });
    expect(keys).toEqual(["Escape"]);

    unmount();
    act(() => {
      window.dispatchEvent(message(EMBED_ESCAPE_MESSAGE, frame.contentWindow));
    });
    expect(keys).toEqual(["Escape"]);
    window.removeEventListener("keydown", onKey, true);
  });
});
