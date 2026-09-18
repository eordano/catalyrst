import { test, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ChromeShell from "./ChromeShell";

const TABS = [
  { id: "overview", label: "Overview", href: "/app" },
  { id: "timeline", label: "Timeline", href: "/app/timeline" },
] as const;

test("tabs are real anchors; without onNavigate or on a modified click the browser keeps the navigation", () => {
  const plain = render(<ChromeShell tabs={TABS} active="overview" tabsLabel="sections" />);
  const tab = screen.getByRole("link", { name: "Timeline" });
  expect(tab.tagName).toBe("A");
  expect(tab).toHaveAttribute("href", "/app/timeline");
  plain.unmount();

  const onNavigate = vi.fn();
  render(<ChromeShell tabs={TABS} active="overview" tabsLabel="sections" onNavigate={onNavigate} />);
  const evt = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0, metaKey: true });
  screen.getByRole("link", { name: "Timeline" }).dispatchEvent(evt);
  expect(onNavigate).not.toHaveBeenCalled();
  expect(evt.defaultPrevented).toBe(false);
});

test("a plain left-click on a tab calls onNavigate, while a button tab calls onTab and never onNavigate", async () => {
  const user = userEvent.setup();
  const onNavigate = vi.fn();
  const onTab = vi.fn();
  render(
    <ChromeShell
      tabs={[...TABS, { id: "console", label: "Console" }]}
      active="overview"
      tabsLabel="sections"
      onTab={onTab}
      onNavigate={onNavigate}
    />,
  );
  await user.click(screen.getByRole("link", { name: "Timeline" }));
  expect(onNavigate).toHaveBeenCalledWith("/app/timeline");
  await user.click(screen.getByRole("button", { name: "Console" }));
  expect(onTab).toHaveBeenCalledWith("console");
  expect(onNavigate).toHaveBeenCalledTimes(1);
});
