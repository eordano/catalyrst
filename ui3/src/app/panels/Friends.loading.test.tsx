import { act, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, test } from "vitest";
import FriendsPanel from "./Friends.route";
import { useBridgeState } from "../../overlay/bridge";
import { FakeBridge, makeFriend, makeFriendRequest } from "../../test/fakeBridge";

afterEach(() => { delete window.dclBridge; });

test("waits for the social snapshot, reuses it when reopening, and clears it when accounts change", () => {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  function Host({ open }: { open: boolean }) {
    useBridgeState();
    return <MemoryRouter>{open && <FriendsPanel floating />}</MemoryRouter>;
  }
  const view = render(<Host open={false} />);
  act(() => bridge.pushIdentity());
  view.rerender(<Host open />);
  expect(screen.getByRole("status")).toHaveTextContent("Connecting to friends");
  expect(screen.queryByText("No friends")).toBeNull();
  act(() => bridge.pushFriends());
  expect(screen.getAllByText("No friends").length).toBeGreaterThan(0);
  view.rerender(<Host open={false} />);
  act(() => bridge.pushFriends({ friends: [makeFriend()], received: [makeFriendRequest()] }));
  view.rerender(<Host open />);
  expect(screen.getByText("Ripley")).toBeInTheDocument();
  expect(screen.queryByText("Connecting to friends\u2026")).toBeNull();
  act(() => bridge.pushIdentity({ address: "0x2222222222222222222222222222222222222222" }));
  expect(screen.queryByText("Ripley")).toBeNull();
  expect(screen.getByRole("status")).toHaveTextContent("Connecting to friends");
  act(() => bridge.pushIdentity({ isGuest: true }));
  expect(screen.getByText("Friends aren't available")).toBeInTheDocument();
});
