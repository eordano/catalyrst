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

// the label is sentence case in the DOM and uppercased by CSS, so the accessible
// name stays readable to assistive tech
const jumpIn = () => screen.getByRole("button", { name: "Continue as guest" });
const signIn = () => screen.getByRole("button", { name: "Sign in" });

afterEach(() => {
  signOutEngineAuth();
});

describe("LobbyNew sign-in affordance", () => {
  test("signed-out: Sign in opens the SignInFlow modal", async () => {
    render(<LobbyNew />);
    await userEvent.click(signIn());
    const modal = await screen.findByRole("dialog", {
      name: "Sign in to Decentraland",
    });
    expect(modal).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /continue with wallet/i }),
    ).toBeTruthy();
  });

  test("signed-in (stashed identity): shows address + Sign out", async () => {
    expect(loginWithIdentity(makeIdentity())).toBe(true);
    render(<LobbyNew />);
    expect(screen.getByText(/Signing in as/)).toBeTruthy();
    expect(screen.getByText("0x0000\u{2026}00aa")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(signIn()).toBeTruthy();
  });
});

describe("LobbyNew call-to-action weighting", () => {
  test("both calls to action are the same button", () => {
    render(<LobbyNew />);
    expect(jumpIn().className).toContain("lobbynew__btn");
    expect(signIn().className).toContain("lobbynew__btn");
  });

  test("signing in leads while the guest identity is untouched", () => {
    render(<LobbyNew />);
    expect(signIn().className).toContain("is-primary");
    expect(jumpIn().className).toContain("is-secondary");
  });

  test("naming the avatar hands the lead to jumping in", async () => {
    render(<LobbyNew />);
    await userEvent.type(screen.getByLabelText("Username"), "x");
    expect(jumpIn().className).toContain("is-primary");
    expect(signIn().className).toContain("is-secondary");
  });

  test("rolling a new name hands the lead to jumping in", async () => {
    render(<LobbyNew />);
    await userEvent.click(screen.getByRole("button", { name: "Random name" }));
    expect(jumpIn().className).toContain("is-primary");
    expect(signIn().className).toContain("is-secondary");
  });

  test("restyling the avatar hands the lead to jumping in", async () => {
    render(<LobbyNew />);
    await userEvent.click(screen.getByRole("radio", { name: "Feminine body" }));
    expect(jumpIn().className).toContain("is-primary");
    expect(signIn().className).toContain("is-secondary");
  });

  // reading the terms is not investment in the guest identity, and must not
  // reorder the two offers under the pointer
  test("agreeing to the terms leaves the lead alone", async () => {
    render(<LobbyNew />);
    await userEvent.click(screen.getByRole("checkbox"));
    expect(signIn().className).toContain("is-primary");
    expect(jumpIn().className).toContain("is-secondary");
  });

  test("a signed-in visitor sees jumping in lead, with no sign-in button", () => {
    expect(loginWithIdentity(makeIdentity())).toBe(true);
    render(<LobbyNew />);
    expect(jumpIn().className).toContain("is-primary");
    expect(screen.queryByRole("button", { name: "Sign in" })).toBeNull();
  });
});
