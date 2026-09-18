import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import CreatorHubChrome from "./CreatorHubChrome";
import { ChromeAuthContext, type ChromeAuth } from "../../web/frames/chrome-auth";

const ACCOUNT = "0x4d02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf2";

function auth(over: Partial<ChromeAuth> = {}): ChromeAuth {
  return {
    signedIn: true,
    account: ACCOUNT,
    mana: "",
    name: "esteban",
    committee: false,
    onSignIn: vi.fn(),
    onSignOut: vi.fn(),
    ...over,
  };
}

afterEach(() => cleanup());

describe("creator hub account entry", () => {
  test("the rail account button opens the menu with Settings and Log out", async () => {
    const a = auth();
    render(
      <ChromeAuthContext.Provider value={a}>
        <CreatorHubChrome active="home" />
      </ChromeAuthContext.Provider>,
    );
    const triggers = screen.getAllByRole("button", { name: "esteban 0x4d02\u{2026}7bf2" });
    expect(triggers.length).toBeGreaterThan(0);
    await userEvent.click(triggers[triggers.length - 1]!);
    expect(screen.getByRole("menuitem", { name: "Settings" })).toHaveAttribute("href", "/creator-hub/settings");
    expect(screen.getByRole("menuitem", { name: "Switch account" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("menuitem", { name: "Log out" }));
    expect(a.onSignOut).toHaveBeenCalledTimes(1);
  });

  test("an explicit onAccount handler wins over the menu, and a signed-out rail offers Sign in", async () => {
    const onAccount = vi.fn();
    render(
      <ChromeAuthContext.Provider value={auth()}>
        <CreatorHubChrome active="home" onAccount={onAccount} />
      </ChromeAuthContext.Provider>,
    );
    const triggers = screen.getAllByRole("button", { name: "esteban 0x4d02\u{2026}7bf2" });
    await userEvent.click(triggers[triggers.length - 1]!);
    expect(onAccount).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    cleanup();

    const a = auth({ signedIn: false, account: "", name: "" });
    render(
      <ChromeAuthContext.Provider value={a}>
        <CreatorHubChrome active="home" />
      </ChromeAuthContext.Provider>,
    );
    await userEvent.click(screen.getAllByRole("button", { name: "Sign in" })[0]!);
    expect(a.onSignIn).toHaveBeenCalledTimes(1);
  });
});
