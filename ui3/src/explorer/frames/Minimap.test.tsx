import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const ownerState = vi.hoisted(() => ({
  address: null as string | null,
  sceneTitle: null as string | null,
  loading: false,
  world: false,
}));

vi.mock("../components/SceneOwnerActions", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../components/SceneOwnerActions")>();
  return { ...mod, useSceneOwner: () => ownerState };
});

import Minimap, { jumpUrl } from "./Minimap";

const OWNER = "0x92de52247aeae00fcfb18072c8564f3549b64f9c";

beforeEach(() => {
  ownerState.address = null;
  ownerState.sceneTitle = null;
  ownerState.loading = false;
  ownerState.world = false;
});

async function openMenu() {
  const user = userEvent.setup();
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <Minimap place="CBD Plaza" coords="-143,102" />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  return user;
}

describe("Minimap scene options", () => {
  it("offers copy coordinates and copy jump link (no jump/twitter entries), copying the bare parcel and the play deep link", async () => {
    const user = await openMenu();
    const names = screen.getAllByRole("menuitem").map((el) => el.textContent ?? "");
    expect(names.some((n) => n.startsWith("Copy coordinates"))).toBe(true);
    expect(names.some((n) => n.startsWith("Copy jump link"))).toBe(true);
    expect(names.some((n) => /Jump to coordinates/i.test(n))).toBe(false);
    expect(names.some((n) => /Twitter/i.test(n))).toBe(false);

    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    await user.click(screen.getByRole("menuitem", { name: /^Copy coordinates/ }));
    expect(writeText).toHaveBeenLastCalledWith("-143,102");
    await user.click(screen.getByRole("button", { name: "Scene options" }));
    await user.click(screen.getByRole("menuitem", { name: /^Copy jump link/ }));
    expect(writeText).toHaveBeenLastCalledWith(`${window.location.origin}/play/?position=-143,102`);
    expect(writeText).toHaveBeenCalledTimes(2);

    expect(jumpUrl("-143,102")).toBe(`${window.location.origin}/play/?position=-143,102`);
    expect(jumpUrl(" 7 , -9 ")).toBe(`${window.location.origin}/play/?position=7,-9`);
    const shared = new URL(jumpUrl("7,-9", "https://worlds.example/test.dcl.eth"));
    expect(shared.searchParams.get("realm")).toBe("https://worlds.example/test.dcl.eth");
    expect(shared.searchParams.get("position")).toBe("7,-9");
    expect(jumpUrl("garbage")).toBe(`${window.location.origin}/play/?position=0,0`);
  });

  it("opens recipient details even when no address is available, while disabling tips", async () => {
    const user = await openMenu();
    expect(screen.getByRole("menuitem", { name: /^Send feedback/ })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: /^Send tip/ })).toBeDisabled();
    await user.click(screen.getByRole("menuitem", { name: /^Send feedback/ }));
    expect(await screen.findByRole("dialog", { name: "Send feedback" })).toBeInTheDocument();
    expect(screen.queryAllByRole("radio")).toHaveLength(0);
  });

  it("opens the feedback and tip dialogs for the resolved owner", async () => {
    ownerState.address = OWNER;
    const user = await openMenu();
    const feedback = screen.getByRole("menuitem", { name: /^Send feedback/ });
    expect(feedback).toBeEnabled();
    await user.click(feedback);
    expect(await screen.findByRole("dialog", { name: "Send feedback" })).toBeInTheDocument();
    expect(await screen.findByText("0x92de\u{2026}4f9c")).toBeInTheDocument();
    expect(screen.getAllByRole("radio")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Close" }));

    await user.click(screen.getByRole("button", { name: "Scene options" }));
    await user.click(screen.getByRole("menuitem", { name: /^Send tip/ }));
    expect(await screen.findByRole("dialog", { name: "Send tip" })).toBeInTheDocument();
  });
});
