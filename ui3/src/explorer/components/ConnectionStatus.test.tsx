import { afterEach, describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { renderToString } from "react-dom/server";

import ConnectionStatus from "./ConnectionStatus";

const ABOUT = {
  realmName: "dcl-one",
  commsProtocol: "v3",
  commsAdapter: "archipelago:archipelago:wss://catalyst.example.com/ws",
  commsVersion: "24.18.0",
  contentVersion: "8.0.3",
  lambdasVersion: "4.12.0",
  usersCount: 3,
};

const PINNED = {
  at: new Date("2026-09-17T17:01:29.000Z"),
  userAgent: "Mozilla/5.0 pinned",
  overlayBuild: "AppShell-CSUtUagR",
};

type Props = Parameters<typeof ConnectionStatus>[0];

function props(over: Partial<Props> = {}): Props {
  return {
    connection: { sceneHealth: "ok", sceneRoom: true, globalRoom: true },
    realm: "dcl-one",
    fps: { page: 58, engine: 61, ms: 17.2 },
    about: ABOUT,
    sceneId: "bafkreiscene",
    ...PINNED,
    ...over,
  };
}

function mount(over: Partial<Props> = {}) {
  return render(<ConnectionStatus {...props(over)} />);
}

function rowValue(label: string): string | null {
  if (!["Page frame rate", "Page frame interval"].includes(label)) fireEvent.click(screen.getByRole("tab", { name: "Connection" }));
  const dt = screen.getByText(label, { selector: "dt" });
  return dt.nextElementSibling?.textContent ?? null;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ConnectionStatus", () => {
  test("keeps connection status without duplicating realm or room facts", () => {
    mount();
    fireEvent.click(screen.getByRole("tab", { name: "Connection" }));
    for (const title of ["Scene", "Scene Room", "Global Room"]) {
      expect(screen.getByText(title, { selector: ".xcs__rowtitle" })).toBeTruthy();
    }
    expect(screen.getAllByText("Connected")).toHaveLength(2);
    expect(screen.queryByText("Realm", { selector: ".xcs__rowtitle" })).toBeNull();
    expect(screen.queryByText("Scene health", { selector: "dt" })).toBeNull();
  });

  test("prioritizes engine fps and labels page timing separately", () => {
    mount();
    const fps = screen.getByTestId("xcs-fps");
    expect(fps.className).toContain("is-good");
    expect(fps.querySelector(".xcs__fpsnum")?.textContent).toBe("61");
    expect(rowValue("Page frame rate")).toBe("58 fps");
    expect(rowValue("Page frame interval")).toBe("17.2 ms");
    expect(screen.getByText(/not GPU render time/)).toBeTruthy();
  });

  test("grades a slow page as bad and hides the engine reading without an engine", () => {
    mount({ fps: { page: 12, engine: null, ms: 83.1 } });
    const fps = screen.getByTestId("xcs-fps");
    expect(fps.className).toContain("is-bad");
    expect(fps.querySelector(".xcs__fpsengine")).toBeNull();
    expect(fps.querySelector(".xcs__fpsnum")?.textContent).toBe("\u{2014}");
    expect(rowValue("Page frame rate")).toBe("12 fps");
  });

  test("renders the debug block from the realm, scene and comms facts", () => {
    mount();
    expect(rowValue("Realm")).toBe("dcl-one");
    expect(rowValue("Scene id")).toBe("bafkreiscene");
    expect(rowValue("Comms")).toBe("v3 \u00b7 archipelago:archipelago:wss://catalyst.example.com/ws");
    expect(rowValue("Server")).toBe("content 8.0.3 \u00b7 lambdas 4.12.0 \u00b7 comms 24.18.0");
    expect(screen.queryByText("FPS", { selector: "dt" })).toBeNull();
  });

  test("shows build but leaves browser and capture time in copied diagnostics only", () => {
    mount();
    expect(rowValue("Overlay build")).toBe("AppShell-CSUtUagR");
    expect(screen.queryByText("Browser", { selector: "dt" })).toBeNull();
    expect(screen.queryByText("Captured", { selector: "dt" })).toBeNull();
  });

  test("reads build, browser and capture time from the client only after mount", () => {
    mount({ at: undefined, userAgent: undefined, overlayBuild: undefined });
    expect(rowValue("Overlay build")).toBe("ConnectionStatus");
    expect(screen.queryByText("Browser", { selector: "dt" })).toBeNull();
    expect(screen.queryByText("Captured", { selector: "dt" })).toBeNull();
  });

  test("omits a null browser row", () => {
    mount({ userAgent: null });
    expect(screen.queryByText("Browser", { selector: "dt" })).toBeNull();
  });

  test("server markup carries no browser, build or capture time", () => {
    const html = renderToString(
      <ConnectionStatus {...props({ at: undefined, userAgent: undefined, overlayBuild: undefined })} />,
    );
    expect(html).toContain("Performance");
    expect(html).toContain("Connection");
    expect(html).not.toContain("Browser");
    expect(html).not.toContain("Overlay build");
    expect(html).not.toContain("Captured");
    expect(html).not.toContain("Node.js");
  });

  test("copies the debug info as text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    mount();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy debug info" }));
    });
    expect(writeText).toHaveBeenCalledTimes(1);
    const text = String(writeText.mock.calls[0]?.[0]);
    expect(text).toContain("Realm: dcl-one");
    expect(text).toContain("Scene id: bafkreiscene");
    expect(text).toContain("FPS: page 58 \u00b7 engine 61 \u00b7 17.2 ms/frame");
    expect(text).toContain("Browser: Mozilla/5.0 pinned");
    expect(text).toContain("Captured: 2026-09-17T17:01:29.000Z");
    expect(screen.getByRole("button", { name: "Copied" })).toBeTruthy();
  });

  test("stamps an unpinned copy with the time of copying", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-09-17T18:00:00.000Z"));
    mount({ at: undefined });
    expect(screen.queryByText("Captured", { selector: "dt" })).toBeNull();
    vi.setSystemTime(new Date("2026-09-17T18:00:05.000Z"));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy debug info" }));
    });
    vi.useRealTimers();
    expect(String(writeText.mock.calls[0]?.[0])).toContain("Captured: 2026-09-17T18:00:05.000Z");
    expect(screen.queryByText("Captured", { selector: "dt" })).toBeNull();
  });

  test("reports a clipboard failure on the button", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    mount();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy debug info" }));
    });
    expect(screen.getByRole("button", { name: "Copy failed" })).toBeTruthy();
  });

  test("closes through the header button", () => {
    const onClose = vi.fn();
    mount({ onClose });
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("ConnectionStatus realm /about", () => {
  const wire = (realmName: string, contentVersion: string) => ({
    configurations: { realmName },
    content: { version: contentVersion },
    comms: { protocol: "v3" },
  });

  function stubAbout(body: unknown) {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL) =>
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  async function settle(fetchMock: ReturnType<typeof stubAbout>) {
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test("fetches /about from the realm url the engine reports, not the page origin", async () => {
    const fetchMock = stubAbout(wire("peer-example", "9.9.9"));
    mount({ about: undefined, realm: "https://peer.example.org/about" });
    await settle(fetchMock);
    expect(String(fetchMock.mock.calls[0]?.[0])).toBe("https://peer.example.org/about");
    expect(rowValue("Server")).toBe("content 9.9.9");
  });

  test("a named realm reads the page-origin /about and keeps it only when it names that realm", async () => {
    const other = stubAbout(wire("somewhere-else", "1.0.0"));
    const view = mount({ about: undefined, realm: "dcl-one" });
    await settle(other);
    expect(String(other.mock.calls[0]?.[0])).toBe(`${window.location.origin}/about`);
    expect(screen.queryByText("Server", { selector: "dt" })).toBeNull();
    view.unmount();

    const same = stubAbout(wire("dcl-one", "8.0.3"));
    mount({ about: undefined, realm: "dcl-one" });
    await settle(same);
    expect(rowValue("Server")).toBe("content 8.0.3");
  });
});
