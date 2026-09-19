import { describe, test, expect, vi, onTestFinished } from "vitest";
import { act, fireEvent, screen, within } from "@testing-library/react";

import { renderHud } from "./harness";
import { anchorBeside, PANEL_MARGIN } from "../explorer/components/FloatingPanel";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const chrome = () => screen.getByRole("dialog", { name: "Explore" });

describe("fullscreen panels", () => {
  test("Settings leaves the scene visible; Backpack and Places use ExploreChrome; unknown routes fall back to the world", async () => {
    const { user, path, navigate } = renderHud();
    expect(path()).toBe("/");
    expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();

    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    expect(path()).toBe("/settings");
    expect(screen.getByRole("region", { name: "Explorer settings" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();
    expect(screen.queryByRole("navigation", { name: "Main menu" })).toBeNull();
    expect(await screen.findByRole("tablist", { name: "Settings sections" })).toBeInTheDocument();

    for (const [button, route] of [["Backpack", "/backpack"], ["Places", "/places"]] as const) {
      await user.keyboard("{Escape}");
      await user.click(within(sidebar()).getByRole("button", { name: button }));
      expect(path()).toBe(route);
      expect(chrome()).toBeInTheDocument();
    }

    await navigate("/definitely-not-a-panel");
    expect(path()).toBe("/");
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
  });

  test("Escape, the close button and the active chrome tab all return to the world HUD; chrome tabs switch panels", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByText("Nearby voice")).toBeInTheDocument();
    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    expect(path()).toBe("/settings");
    await user.keyboard("{Escape}");
    expect(path()).toBe("/");
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Explore" })).toBeNull();
    expect(screen.queryByText("Nearby voice")).toBeNull();

    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));
    await user.click(screen.getByRole("button", { name: "Back to world" }));
    expect(path()).toBe("/");

    await user.click(within(sidebar()).getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));
    expect(path()).toBe("/places");
    await user.click(within(chrome()).getByRole("button", { name: /Places/ }));
    expect(path()).toBe("/");
  });

  test("hotkeys route to panels but are ignored while typing in the chat input", async () => {
    const { user, path } = renderHud();
    await user.keyboard("p");
    expect(path()).toBe("/settings");
    await user.keyboard("{Escape}");
    await user.keyboard("m");
    expect(path()).toBe("/map");
    await user.keyboard("m");
    expect(path()).toBe("/");

    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    const input = screen.getByLabelText("Send a message to Nearby chat");
    await user.type(input, "m");
    expect(path()).toBe("/");
    expect(input).toHaveValue("m");
  });

  test("the ExploreChrome user chip opens the profile menu, which routes to passport", async () => {
    const { user, path } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Places" }));

    const chip = within(chrome()).getByRole("button", { name: /Guest/ });
    await user.click(chip);
    expect(chip).toHaveAttribute("aria-expanded", "true");

    await user.click(screen.getByRole("button", { name: "VIEW PROFILE" }));
    expect(path()).toBe("/passport");
  });
});

type RectSpec = { top: number; height: number };

function rectOf({ top, height }: RectSpec): DOMRect {
  return { x: 0, y: top, top, bottom: top + height, left: 0, right: 0, width: 0, height, toJSON: () => ({}) } as DOMRect;
}

function stubRects(pick: (el: Element) => RectSpec | null): void {
  const original = Element.prototype.getBoundingClientRect;
  const spy = vi
    .spyOn(Element.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: Element) {
      const spec = pick(this);
      return spec ? rectOf(spec) : original.call(this);
    });
  onTestFinished(() => spy.mockRestore());
}

const isSidebarButton = (el: Element, id: string) => el.getAttribute("data-sb-panel") === id;
const isPanel = (el: Element, id: string) => el.getAttribute("data-hud-panel") === id;

const FLOATING: { button: string; id: string; title: string }[] = [
  { button: "Voice Chat", id: "voice", title: "Nearby voice" },
  { button: "Portable Experiences", id: "portables", title: "Portable experiences" },
  { button: "Skybox", id: "skybox", title: "Time of day" },
  { button: "Friends", id: "friends", title: "Friends" },
  { button: "Notifications", id: "notifications", title: "Notifications" },
  { button: "Chat", id: "chat", title: "Chat" },
];

describe("floating panels", () => {
  test("scrolling portable experiences does not remeasure or remove its height constraint", async () => {
    const { user } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Portable Experiences" }));
    const panel = await screen.findByRole("region", { name: "Portable experiences" });
    const body = panel.querySelector<HTMLElement>(".fp__body")!;
    const maxHeight = panel.style.maxHeight;
    const measure = vi.spyOn(panel, "getBoundingClientRect").mockImplementation(() => {
      expect(panel.style.maxHeight).toBe(maxHeight);
      return rectOf({ top: 16, height: 400 });
    });
    onTestFinished(() => measure.mockRestore());
    body.scrollTop = 160;
    fireEvent.scroll(body);
    expect(measure).not.toHaveBeenCalled();
    expect(body.scrollTop).toBe(160);
    fireEvent(window, new Event("resize"));
    expect(measure).toHaveBeenCalledTimes(1);
    expect(body.scrollTop).toBe(160);
  });
  test("floating panels share one surface and close control; chat keeps its bottom anchor", async () => {
    const tops = new Map(FLOATING.map((f, i) => [f.id, 120 + i * 90]));
    stubRects((el) => {
      const hit = FLOATING.find((f) => isSidebarButton(el, f.id));
      return hit ? { top: tops.get(hit.id)!, height: 38 } : null;
    });
    const { user, container } = renderHud();
    const visiblePanels = () => [...container.querySelectorAll("[data-hud-panel]")].filter((el) => !el.closest("[hidden]"));
    expect(visiblePanels()).toHaveLength(0);

    for (const { button, id, title } of FLOATING) {
      const top = tops.get(id)!;
      await user.click(within(sidebar()).getByRole("button", { name: button }));
      const panel = await screen.findByRole("region", { name: title });
      expect(panel).toHaveAttribute("data-hud-panel", id);
      if (id === "chat") {
        expect(panel.style.top).toBe("");
        expect(panel.style.maxHeight).toBe("");
      } else {
        expect(panel.style.top).toBe(`${top}px`);
        expect(panel.style.maxHeight).toBe(`${window.innerHeight - PANEL_MARGIN - top}px`);
      }
      expect(within(panel).getByRole("heading", { name: title })).toBeInTheDocument();
      expect(visiblePanels()).toHaveLength(1);
      expect(within(panel).getAllByRole("button", { name: /^Close(?: chat)?$/ })).toHaveLength(1);
      await user.click(within(panel).getByRole("button", { name: /^Close(?: chat)?$/ }));
      expect(visiblePanels()).toHaveLength(0);
    }
  });

  test("a bottom button flips the panel upward and the panel re-anchors on resize", async () => {
    const vh = window.innerHeight;
    const btn = { top: vh - 68, height: 38 };
    let voiceTop = 200;
    stubRects((el) =>
      isSidebarButton(el, "friends") ? btn
        : isPanel(el, "friends") ? { top: 0, height: 400 }
        : isSidebarButton(el, "voice") ? { top: voiceTop, height: 38 }
        : null,
    );
    const { user, container } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    const friends = await screen.findByRole("region", { name: "Friends" });
    const expectedTop = btn.top + btn.height - 400;
    expect(friends.style.top).toBe(`${expectedTop}px`);
    expect(friends.style.maxHeight).toBe(`${vh - PANEL_MARGIN - expectedTop}px`);

    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    const voice = container.querySelector('[data-hud-panel="voice"]') as HTMLElement;
    expect(voice.style.top).toBe("200px");
    voiceTop = 260;
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(voice.style.top).toBe("260px");
  });
});

describe("anchorBeside", () => {
  test("aligns below the button when it fits, flips upward on overflow, clamps to the margin, and falls back without a button", () => {
    expect(anchorBeside({ top: 100, bottom: 138, height: 38 }, 300, 800)).toEqual({ top: 100, maxHeight: 684 });
    expect(anchorBeside({ top: 700, bottom: 738, height: 38 }, 300, 800)).toEqual({ top: 438, maxHeight: 346 });
    expect(anchorBeside({ top: 700, bottom: 738, height: 38 }, 900, 800)).toEqual({ top: 16, maxHeight: 768 });
    expect(anchorBeside({ top: 82, bottom: 120, height: 38 }, 868, 900)).toEqual({ top: 82, maxHeight: 802 });
    expect(anchorBeside(null, 300, 800)).toEqual({ top: 16, maxHeight: 768 });
    expect(anchorBeside({ top: 0, bottom: 0, height: 0 }, 300, 800)).toEqual({ top: 16, maxHeight: 768 });
  });
});


test("clicking the world dismisses sidebar options and tooltips without restoring sidebar focus", async () => {
  const { user } = renderHud();
  const options = within(sidebar()).getByRole("button", { name: "More options" });
  await user.click(options);
  expect(options).toHaveAttribute("aria-expanded", "true");
  const world = document.createElement("canvas");
  world.tabIndex = 0;
  document.body.appendChild(world);
  onTestFinished(() => world.remove());
  await user.click(world);
  expect(options).toHaveAttribute("aria-expanded", "false");
  expect(options).not.toHaveFocus();
  expect(screen.queryByRole("tooltip", { name: "More options" })).toBeNull();
  await user.hover(options);
  expect(screen.getByRole("tooltip", { name: "More options" })).toBeInTheDocument();
  fireEvent(document, new Event("pointerlockchange"));
  expect(screen.queryByRole("tooltip", { name: "More options" })).toBeNull();
});
