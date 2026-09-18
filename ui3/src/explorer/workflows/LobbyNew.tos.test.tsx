import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import LobbyNew from "./LobbyNew";

const checks = (root: HTMLElement) =>
  root.querySelector(".lobbynew__checks")?.className ?? "";

const TRIGGERS: Array<[string, "button" | "radio"]> = [
  ["Random name", "button"],
  ["Random", "button"],
  ["Feminine body", "radio"],
];

describe("LobbyNew terms nudge", () => {
  test("the terms sit quiet until typing raises them once, and accepting the terms clears the nudge", async () => {
    const { container } = render(<LobbyNew />);
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();

    const field = screen.getByLabelText("Username");
    await userEvent.type(field, "a");
    expect(checks(container)).toContain("is-nudged");
    expect(screen.getByRole("alert").textContent).toMatch(/accept the terms/i);
    await userEvent.type(field, "bc");
    expect(screen.getAllByRole("alert")).toHaveLength(1);

    await userEvent.click(screen.getByRole("checkbox"));
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("the random-name and body-shape controls raise the terms too", async () => {
    for (const [name, role] of TRIGGERS) {
      const view = render(<LobbyNew />);
      await userEvent.click(screen.getByRole(role, { name }));
      expect(checks(view.container), name).toContain("is-nudged");
      expect(screen.getByRole("alert").textContent, name).toMatch(/accept the terms/i);
      view.unmount();
    }
  });

  test("with the terms already accepted, neither typing nor the other controls raise anything", async () => {
    const typed = render(<LobbyNew />);
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.type(screen.getByLabelText("Username"), "a");
    expect(checks(typed.container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
    typed.unmount();

    for (const [name, role] of TRIGGERS) {
      const view = render(<LobbyNew />);
      await userEvent.click(screen.getByRole("checkbox"));
      await userEvent.click(screen.getByRole(role, { name }));
      expect(checks(view.container), name).not.toContain("is-nudged");
      expect(screen.queryByRole("alert"), name).toBeNull();
      view.unmount();
    }
  });
});
