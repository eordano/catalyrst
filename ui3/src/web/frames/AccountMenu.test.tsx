import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import DclTopBar from "./DclTopBar";
import { ChromeAuthContext, type ChromeAuth } from "./chrome-auth";

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

function renderBar(a: ChromeAuth) {
  return render(
    <ChromeAuthContext.Provider value={a}>
      <DclTopBar variant="sites" />
    </ChromeAuthContext.Provider>,
  );
}

const menu = () => screen.queryByRole("menu", { name: "My account" });
const openMenu = () => userEvent.click(screen.getByRole("button", { name: "My account" }));

afterEach(() => cleanup());

describe("top bar profile menu", () => {
  test("the avatar opens a menu with the account; Escape closes it; Switch account signs out then back in; Log out signs out and closes", async () => {
    const a = auth();
    renderBar(a);
    expect(menu()).toBeNull();

    await openMenu();
    expect(menu()).toHaveTextContent("esteban");
    expect(menu()).toHaveTextContent("0x4d02\u{2026}7bf2");
    expect(screen.getByRole("menuitem", { name: "My assets" })).toHaveAttribute("href", "/marketplace/account");
    await userEvent.keyboard("{Escape}");
    expect(menu()).toBeNull();

    await openMenu();
    await userEvent.click(screen.getByRole("menuitem", { name: "Switch account" }));
    expect(a.onSignOut).toHaveBeenCalledTimes(1);
    expect(a.onSignIn).toHaveBeenCalledTimes(1);

    await openMenu();
    await userEvent.click(screen.getByRole("menuitem", { name: "Log out" }));
    expect(a.onSignOut).toHaveBeenCalledTimes(2);
    expect(menu()).toBeNull();
  });

  test("signed out shows SIGN IN instead of the avatar, and Learn > Docs links to the self-hosted mirror", async () => {
    const a = auth({ signedIn: false, account: "", name: "" });
    renderBar(a);
    expect(screen.queryByRole("button", { name: "My account" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "SIGN IN" }));
    expect(a.onSignIn).toHaveBeenCalledTimes(1);

    const docs = screen.getAllByRole("menuitem", { name: "Docs", hidden: true })[0];
    expect(docs).toHaveAttribute("href", "/docs/");
  });
});
