import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import LobbyNew from "./LobbyNew";

const checks = (root: HTMLElement) =>
  root.querySelector(".lobbynew__checks")?.className ?? "";

describe("LobbyNew terms nudge", () => {
  test("the terms sit quiet until something asks for them", () => {
    const { container } = render(<LobbyNew />);
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("typing a name raises the terms requirement", async () => {
    const { container } = render(<LobbyNew />);
    await userEvent.type(screen.getByLabelText("Username"), "a");
    expect(checks(container)).toContain("is-nudged");
    expect(screen.getByRole("alert").textContent).toMatch(/accept the terms/i);
  });

  test("typing a name with the terms already accepted raises nothing", async () => {
    const { container } = render(<LobbyNew />);
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.type(screen.getByLabelText("Username"), "a");
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("accepting the terms clears a nudge that typing raised", async () => {
    const { container } = render(<LobbyNew />);
    await userEvent.type(screen.getByLabelText("Username"), "a");
    expect(screen.getByRole("alert")).toBeTruthy();

    await userEvent.click(screen.getByRole("checkbox"));
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  // the alert is assertive, so it must mount once and stay put rather than
  // re-announcing on every keystroke
  test("further typing does not re-raise the alert", async () => {
    render(<LobbyNew />);
    const field = screen.getByLabelText("Username");
    await userEvent.type(field, "abc");
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  // every control that flips the CTA emphasis must also raise the terms, or the two
  // signals disagree about whether the visitor has committed to the guest path
  test.each([
    ["Random name", "button"],
    ["Random", "button"],
    ["Feminine body", "radio"],
  ])("%s raises the terms too", async (name, role) => {
    const { container } = render(<LobbyNew />);
    await userEvent.click(screen.getByRole(role, { name }));
    expect(checks(container)).toContain("is-nudged");
    expect(screen.getByRole("alert").textContent).toMatch(/accept the terms/i);
  });

  test.each([
    ["Random name", "button"],
    ["Random", "button"],
    ["Feminine body", "radio"],
  ])("%s raises nothing once the terms are accepted", async (name, role) => {
    const { container } = render(<LobbyNew />);
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.click(screen.getByRole(role, { name }));
    expect(checks(container)).not.toContain("is-nudged");
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
