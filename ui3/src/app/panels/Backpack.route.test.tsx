import { act, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { FakeBridge } from "../../test/fakeBridge";

type Outfit = { bodyShape?: string; wearables?: string[] };

const owned = vi.hoisted(() => ({
  isLoading: true,
  equipped: null as null | { bodyShape: string; wearables: string[]; emotes: string[] },
}));

const preview = vi.hoisted(() => ({
  outfits: [] as Outfit[],
  onStatus: null as null | ((s: string) => void),
}));

vi.mock("../../data/hooks/useOwnedItems", () => ({
  prefetchOwnedItems: () => {},
  useOutfits: () => ({ data: [] }),
  useOwnedItems: () => ({
    wearables: { data: owned.equipped ? { equipped: owned.equipped } : undefined },
    emotes: { data: undefined },
    isLoading: owned.isLoading,
    isError: false,
    error: null,
  }),
}));

vi.mock("../../wearable-preview/WearablePreview", () => ({
  default: (props: { outfit: Outfit; onStatus?: (s: string) => void }) => {
    preview.outfits.push(props.outfit);
    preview.onStatus = props.onStatus ?? null;
    return <div data-testid="wearable-preview" />;
  },
}));

vi.mock("../../explorer/pages/Backpack", () => ({
  default: (props: { avatarPreview: ReactNode }) => <div>{props.avatarPreview}</div>,
}));

import BackpackPanel from "./Backpack.route";

const FEMALE = "urn:decentraland:off-chain:base-avatars:BaseFemale";
const HAT = "urn:decentraland:matic:collections-v2:0xhat:1";

const loading = () => screen.queryByRole("status", { name: "Loading avatar\u{2026}" });

beforeEach(() => {
  owned.isLoading = true;
  owned.equipped = null;
  preview.outfits.length = 0;
  preview.onStatus = null;
});

afterEach(() => {
  delete window.dclBridge;
  vi.restoreAllMocks();
});

describe("Backpack cold open", () => {
  test("keeps the loading state and mounts no preview until the catalyst outfit is known, then boots the preview on that outfit and stays loading until it reports ready", () => {
    const view = render(<BackpackPanel />);
    expect(loading()).not.toBeNull();
    expect(screen.queryByTestId("wearable-preview")).toBeNull();
    expect(preview.outfits).toEqual([]);

    owned.isLoading = false;
    owned.equipped = { bodyShape: FEMALE, wearables: [HAT], emotes: [] };
    view.rerender(<BackpackPanel />);
    expect(screen.getByTestId("wearable-preview")).toBeInTheDocument();
    expect(preview.outfits[0]).toMatchObject({ bodyShape: FEMALE, wearables: [HAT] });
    expect(preview.outfits.every((o) => o.bodyShape === FEMALE)).toBe(true);
    expect(loading()).not.toBeNull();

    act(() => preview.onStatus?.("loading"));
    expect(loading()).not.toBeNull();
    act(() => preview.onStatus?.("ready"));
    expect(loading()).toBeNull();
  });

  test("an engine avatar loadout unblocks the preview before the catalyst query settles", () => {
    const bridge = new FakeBridge();
    window.dclBridge = bridge;
    render(<BackpackPanel />);
    expect(screen.queryByTestId("wearable-preview")).toBeNull();

    act(() => {
      bridge.push({ kind: "avatar", bodyShape: FEMALE, wearables: [HAT], emotes: [] });
    });
    expect(screen.getByTestId("wearable-preview")).toBeInTheDocument();
    expect(preview.outfits[0]).toMatchObject({ bodyShape: FEMALE, wearables: [HAT] });
    expect(loading()).not.toBeNull();
  });
});
