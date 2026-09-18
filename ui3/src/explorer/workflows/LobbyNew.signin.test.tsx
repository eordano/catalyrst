import { afterEach, describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import LobbyNew from "./LobbyNew";
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

const jumpIn = () => screen.getByRole("button", { name: "Continue as guest" });
const signIn = () => screen.getByRole("button", { name: "Sign in" });

afterEach(() => {
  signOutEngineAuth();
});

describe("LobbyNew sign-in affordance", () => {
  test("signed-out: both calls to action are the same button, signing in leads, and Sign in opens the SignInFlow modal", async () => {
    render(<LobbyNew />);
    expect(jumpIn().className).toContain("lobbynew__btn");
    expect(signIn().className).toContain("lobbynew__btn");
    expect(signIn().className).toContain("is-primary");
    expect(jumpIn().className).toContain("is-secondary");

    await userEvent.click(signIn());
    const modal = await screen.findByRole("dialog", {
      name: "Sign in to Decentraland",
    });
    expect(modal).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /continue with wallet/i }),
    ).toBeTruthy();
  });

  test("touching the guest identity (name, random name, body) hands the lead to jumping in; agreeing to the terms leaves it alone", async () => {
    const edits: Array<[string, () => Promise<void>]> = [
      ["typed name", () => userEvent.type(screen.getByLabelText("Username"), "x")],
      ["random name", () => userEvent.click(screen.getByRole("button", { name: "Random name" }))],
      ["body shape", () => userEvent.click(screen.getByRole("radio", { name: "Feminine body" }))],
    ];
    for (const [label, edit] of edits) {
      const view = render(<LobbyNew />);
      await edit();
      expect(jumpIn().className, label).toContain("is-primary");
      expect(signIn().className, label).toContain("is-secondary");
      view.unmount();
    }

    render(<LobbyNew />);
    await userEvent.click(screen.getByRole("checkbox"));
    expect(signIn().className).toContain("is-primary");
    expect(jumpIn().className).toContain("is-secondary");
  });

  test("signed-in (stashed identity): shows the address with jumping in leading and no Sign in button, and Sign out returns to signed-out", async () => {
    expect(loginWithIdentity(makeIdentity())).toBe(true);
    render(<LobbyNew />);
    expect(screen.getByText(/Signing in as/)).toBeTruthy();
    expect(screen.getByText("0x0000\u{2026}00aa")).toBeTruthy();
    expect(jumpIn().className).toContain("is-primary");
    expect(screen.queryByRole("button", { name: "Sign in" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(signIn()).toBeTruthy();
  });
});
