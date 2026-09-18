import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import LobbyNew from "./LobbyNew";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
vi.mock("../../wearable-preview/WearablePreview", () => ({ default: () => null }));
const renderLobby = () => render(<QueryClientProvider client={new QueryClient()}><LobbyNew /></QueryClientProvider>);
import { loginWithIdentity, signOutEngineAuth } from "../../data/auth/engineLogin";

const SIGNER = "0x00000000000000000000000000000000000000aa";

function makeIdentity() {
  const expiration = new Date(Date.now() + 86_400_000).toISOString();
  return {
    signer: SIGNER,
    ephemeral: { address: "0xeph", privateKey: "0xkey" as const },
    expiration,
    authChain: [
      { type: "SIGNER" as const, payload: SIGNER, signature: "" },
      { type: "ECDSA_EPHEMERAL" as const, payload: "msg", signature: "0xsig" },
    ],
  };
}

const guest = () => screen.getByRole("button", { name: "Play as a guest" });
const signIn = () => screen.getByRole("button", { name: "Login or sign up" });

beforeAll(async () => {
  await import("../../overlay/SignInFlow");
});

afterEach(() => {
  signOutEngineAuth();
});

describe("LobbyNew sign-in affordance", () => {
  test("signed-out: the landing offers guest play and login, with no identity form, and login opens the SignInFlow modal", async () => {
    renderLobby();
    expect(screen.getByRole("heading", { name: "Welcome to Decentraland!" })).toBeTruthy();
    expect(guest()).toBeEnabled();
    expect(screen.queryByLabelText("Username")).toBeNull();
    expect(screen.queryByRole("checkbox")).toBeNull();

    await userEvent.click(signIn());
    const modal = await screen.findByRole("dialog", {
      name: "Sign in to Decentraland",
    });
    expect(modal).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /continue with wallet/i }),
    ).toBeTruthy();
  });

  test("playing as a guest opens the name step, and Back returns to the landing keeping what was typed", async () => {
    renderLobby();
    await userEvent.click(guest());
    expect(screen.getByRole("heading", { name: "Pick Your Name" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Login or sign up" })).toBeNull();
    const field = screen.getByLabelText("Username") as HTMLInputElement;
    await userEvent.clear(field);
    await userEvent.type(field, "Ada");

    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(signIn()).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
    await userEvent.click(guest());
    expect((screen.getByLabelText("Username") as HTMLInputElement).value).toBe("Ada");
  });

  test("signed-in (stashed identity): shows the address with entering in progress and no login button, and Sign out returns to signed-out", async () => {
    expect(loginWithIdentity(makeIdentity())).toBe(true);
    renderLobby();
    expect(screen.getByText(/Signing in as/)).toBeTruthy();
    expect(screen.getByText("0x0000\u{2026}00aa")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Entering Decentraland\u2026" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Play as a guest" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Login or sign up" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(signIn()).toBeTruthy();
  });
});
