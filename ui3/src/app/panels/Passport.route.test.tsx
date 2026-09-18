import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";

const state = vi.hoisted(() => ({ target: "", navigate: vi.fn(), avatar: null as any }));
const SELF = "0x1111111111111111111111111111111111111111";
vi.mock("react-router", () => ({ useNavigate: () => state.navigate, useSearchParams: () => [new URLSearchParams(state.target ? { address: state.target } : {})] }));
vi.mock("../../overlay/bridge", () => ({ getBridge: () => null, sendBridge: vi.fn(), useBridgeState: (select: (value: any) => unknown) => select({ identity: { address: SELF, name: "Me" }, avatarLoadout: state.avatar, avatarBase: null }) }));
vi.mock("../../data/hooks/useProfile", () => ({
  resolveSelfAddress: () => SELF,
  usePassport: () => ({ profile: { name: "Explorer", links: [] }, badges: { achieved: [] }, photos: [], isLive: true }),
}));
vi.mock("../../data/hooks/useOwnedItems", () => ({ useOwnedWearables: () => ({ data: { equipped: { bodyShape: "saved-body", wearables: ["saved-shirt"] }, catalog: [] }, isPending: false, refetch: vi.fn() }) }));
vi.mock("../../wearable-preview/WearablePreview", () => ({ default: ({ outfit }: { outfit: unknown }) => <div data-testid="avatar-model">{JSON.stringify(outfit)}</div> }));
vi.mock("../../explorer/pages/Passport", () => ({ default: ({ avatarPreview, onEditAvatar }: { avatarPreview: ReactNode; onEditAvatar: () => void }) => <div>{avatarPreview}<button onClick={onEditAvatar}>Edit avatar</button></div> }));

import PassportPanel from "./Passport.route";

beforeEach(() => { state.target = ""; state.avatar = { bodyShape: "live-body", wearables: ["live-hat"] }; state.navigate.mockClear(); });

test("own profile renders the current engine outfit and opens the backpack", () => {
  render(<PassportPanel />);
  const outfit = JSON.parse(screen.getByTestId("avatar-model").textContent!);
  expect(outfit).toMatchObject({ bodyShape: "live-body", wearables: ["live-hat"] });
  fireEvent.click(screen.getByRole("button", { name: "Edit avatar" }));
  expect(state.navigate).toHaveBeenCalledWith("/backpack");
});

test("another profile renders its saved avatar without using the local outfit", () => {
  state.target = "0x2222222222222222222222222222222222222222";
  render(<PassportPanel />);
  const outfit = JSON.parse(screen.getByTestId("avatar-model").textContent!);
  expect(outfit).toMatchObject({ bodyShape: "saved-body", wearables: ["saved-shirt"] });
});

test("an empty live loadout remains empty instead of restoring saved clothing", () => {
  state.avatar = { bodyShape: "live-body", wearables: [] };
  render(<PassportPanel />);
  expect(JSON.parse(screen.getByTestId("avatar-model").textContent!).wearables).toEqual([]);
});
