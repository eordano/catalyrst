import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import LobbyNew from "./LobbyNew";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
vi.mock("../../wearable-preview/WearablePreview", () => ({ default: () => null }));
type OnJumpIn = NonNullable<Parameters<typeof LobbyNew>[0]>["onJumpIn"];
const renderLobby = async (onJumpIn?: OnJumpIn) => {
  const view = render(<QueryClientProvider client={new QueryClient()}><LobbyNew onJumpIn={onJumpIn} /></QueryClientProvider>);
  await userEvent.click(screen.getByRole("button", { name: "Play as a guest" }));
  return view;
};

const TRIGGERS: Array<[string, "button" | "radio"]> = [
  ["Random name", "button"],
  ["Random", "button"],
  ["Feminine body", "radio"],
];

describe("LobbyNew terms nudge", () => {
  test("the terms sit quiet until typing raises them once, and accepting the terms clears the nudge", async () => {
    await renderLobby();
    expect(screen.queryByRole("alert")).toBeNull();

    const field = screen.getByLabelText("Username");
    await userEvent.type(field, "a");
    expect(screen.getByRole("alert").textContent).toMatch(/accept the terms/i);
    await userEvent.type(field, "bc");
    expect(screen.getAllByRole("alert")).toHaveLength(1);

    await userEvent.click(screen.getByRole("checkbox"));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("the random-name and body-shape controls raise the terms too", async () => {
    for (const [name, role] of TRIGGERS) {
      const view = await renderLobby();
      await userEvent.click(screen.getByRole(role, { name }));
      expect(screen.getByRole("alert").textContent, name).toMatch(/accept the terms/i);
      view.unmount();
    }
  });

  test("entering stays blocked until the terms are accepted, and pressing the blocked button raises them", async () => {
    const onJumpIn = vi.fn();
    const { container } = await renderLobby(onJumpIn);
    const go = screen.getByRole("button", { name: "Let\u{2019}s go" });
    expect(go).toBeDisabled();

    await userEvent.click(container.querySelector(".lobbynew__jumpwrap")!);
    expect(onJumpIn).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(/accept the terms/i);

    await userEvent.click(screen.getByRole("checkbox"));
    expect(go).toBeEnabled();
    await userEvent.click(go);
    expect(onJumpIn).toHaveBeenCalledTimes(1);
    expect(onJumpIn).toHaveBeenCalledWith(expect.objectContaining({ body: "A" }));
  });

  test("with the terms already accepted, neither typing nor the other controls raise anything", async () => {
    const typed = await renderLobby();
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.type(screen.getByLabelText("Username"), "a");
    expect(screen.queryByRole("alert")).toBeNull();
    typed.unmount();

    for (const [name, role] of TRIGGERS) {
      const view = await renderLobby();
      await userEvent.click(screen.getByRole("checkbox"));
      await userEvent.click(screen.getByRole(role, { name }));
      expect(screen.queryByRole("alert"), name).toBeNull();
      view.unmount();
    }
  });
});
