import { test, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ChromeShell from "./ChromeShell";

const TABS = [
  { id: "overview", label: "Overview", href: "/app" },
  { id: "timeline", label: "Timeline", href: "/app/timeline" },
] as const;

test("plain-<a> default: tabs render as real anchors when onNavigate is omitted", () => {
  render(<ChromeShell tabs={TABS} active="overview" tabsLabel="sections" />);
  const tab = screen.getByRole("link", { name: "Timeline" });
  expect(tab.tagName).toBe("A");
  expect(tab).toHaveAttribute("href", "/app/timeline");
});

test("router-owned front: a plain left-click on a tab calls onNavigate and stays client-side", async () => {
  const user = userEvent.setup();
  const onNavigate = vi.fn();
  render(
    <ChromeShell
      tabs={TABS}
      active="overview"
      tabsLabel="sections"
      onNavigate={onNavigate}
    />,
  );
  const tab = screen.getByRole("link", { name: "Timeline" });
  await user.click(tab);
  expect(onNavigate).toHaveBeenCalledWith("/app/timeline");
});

test("a modified click (new-tab intent) is left alone even with onNavigate wired", async () => {
  const onNavigate = vi.fn();
  render(
    <ChromeShell
      tabs={TABS}
      active="overview"
      tabsLabel="sections"
      onNavigate={onNavigate}
    />,
  );
  const tab = screen.getByRole("link", { name: "Timeline" });
  const evt = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    button: 0,
    metaKey: true,
  });
  tab.dispatchEvent(evt);
  expect(onNavigate).not.toHaveBeenCalled();
  expect(evt.defaultPrevented).toBe(false);
});

test("a button tab (no href) still calls onTab and never onNavigate", async () => {
  const user = userEvent.setup();
  const onTab = vi.fn();
  const onNavigate = vi.fn();
  render(
    <ChromeShell
      tabs={[{ id: "console", label: "Console" }]}
      active="console"
      tabsLabel="sections"
      onTab={onTab}
      onNavigate={onNavigate}
    />,
  );
  await user.click(screen.getByRole("button", { name: "Console" }));
  expect(onTab).toHaveBeenCalledWith("console");
  expect(onNavigate).not.toHaveBeenCalled();
});
