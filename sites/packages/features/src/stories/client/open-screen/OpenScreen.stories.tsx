// Storybook-only fixtures: the live loadPlaces/fetchMostActivePlaces loaders do
// not run in Storybook, so these feed representative display data. Production
// paths stay on the real loaders + schema-honesty (see the route loader).
import type { Meta, StoryObj } from "@ui/docs/sb";
import { expect, fn, userEvent, waitFor } from "@ui/docs/sb";

import type { Place } from "@data/lib/catalyst/places/index";
import OpenScreen, { toOpenPlace } from "./OpenScreen";

function fixturePlace(overrides: Partial<Place> & Pick<Place, "id">): Place {
  return {
    title: null,
    description: null,
    image: null,
    owner: null,
    positions: ["0,0"],
    base_position: "0,0",
    updated_at: null,
    created_at: null,
    contact_name: null,
    categories: [],
    highlighted: false,
    highlighted_image: null,
    user_count: null,
    user_visits: 0,
    favorites: 0,
    likes: 0,
    like_rate: null,
    world: false,
    world_name: null,
    ...overrides,
  };
}

const busiestFixture = fixturePlace({
  id: "plc-genesis-plaza",
  title: "Genesis Plaza",
  base_position: "0,0",
  user_count: 132,
  like_rate: 0.97,
  highlighted: true,
  contact_name: "Decentraland Foundation",
});

const liveFixtures: Place[] = [
  busiestFixture,
  fixturePlace({
    id: "plc-exodus-town",
    title: "Exodus Town",
    base_position: "148,60",
    user_count: 41,
    like_rate: 0.9,
    contact_name: "Exodus DAO",
  }),
  fixturePlace({
    id: "plc-vegas-city",
    title: "Vegas City Plaza",
    base_position: "-104,132",
    user_count: 17,
    like_rate: 0.82,
    contact_name: "Vegas City",
  }),
];

const browseFixtures: Place[] = [
  ...liveFixtures,
  fixturePlace({
    id: "plc-wondermine",
    title: "WonderMine Crafting Game",
    base_position: "-29,55",
    user_visits: 5400,
    like_rate: 0.88,
    contact_name: "WonderZone",
  }),
  fixturePlace({
    id: "plc-soho-plaza",
    title: "SoHo Plaza",
    base_position: "52,8",
    like_rate: 0.75,
    contact_name: "SoHo DAO",
  }),
  fixturePlace({
    id: "plc-museum",
    title: "Museum District",
    base_position: "9,77",
    like_rate: 0.7,
    contact_name: "Museum DAO",
  }),
];

const surpriseFixture = toOpenPlace(liveFixtures[1]);

const meta = {
  title: "Sites Specs/client/open-screen/OpenScreen",
  component: OpenScreen,
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  args: {
    arm: "base",
    places: browseFixtures,
    busiest: toOpenPlace(busiestFixture),
    surprise: surpriseFixture,
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "base",
      experimentKey: "client_open_screen",
    },
    track: fn(),
    navigate: fn(),
    jumpDelayMs: 600_000,
  },
} satisfies Meta<typeof OpenScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Base: Story = {
  args: { arm: "base" },
  play: async ({ canvas }) => {
    await canvas.findByRole("heading", { name: "Things to do" });
    // Cards carry no aria-label override, so the place name is part of the
    // visible accessible name -- assert the text, not a label.
    await canvas.findByText("Genesis Plaza");
  },
};

export const Genesis: Story = {
  args: {
    arm: "genesis",
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "genesis",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ args, canvas }) => {
    await canvas.findByRole("heading", {
      name: /^Now entering:/,
    });
    await canvas.findByText(/132 online now/);
    await userEvent.click(await canvas.findByRole("button", { name: "Jump now" }));
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/places/plc-genesis-plaza?from=open-screen",
      ),
    );
    // The primary-metric conversion event must fire on the jump.
    expect(args.track).toHaveBeenCalledWith(
      "cl_open_jumped_in",
      expect.objectContaining({ place_id: "plc-genesis-plaza", variant: "genesis" }),
      expect.anything(),
    );
  },
};

export const GenesisUnavailable: Story = {
  args: {
    arm: "genesis",
    busiest: null,
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "genesis",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ args }) => {
    // No live reading must never dead-end: it redirects straight to Places.
    await waitFor(() => expect(args.navigate).toHaveBeenCalledWith("/places"));
  },
};

export const ThreeCards: Story = {
  args: {
    arm: "three-cards",
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "three-cards",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ canvas }) => {
    await canvas.findByRole("heading", { name: "What do you feel like?" });
    // Titles are visible text (no aria-label override on the cards).
    await canvas.findByText("Jump into the action");
    await canvas.findByText("Surprise me");
    await canvas.findByText("Customize your avatar");
  },
};

// The genesis arm's headline behavior: it auto-jumps the player in after a short
// delay with NO click. jumpDelayMs is tiny here so the timer fires in-test.
export const GenesisAutoJump: Story = {
  args: {
    arm: "genesis",
    jumpDelayMs: 40,
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "genesis",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ args }) => {
    await waitFor(() =>
      expect(args.navigate).toHaveBeenCalledWith(
        "/places/plc-genesis-plaza?from=open-screen",
      ),
    );
  },
};

// Regression for the auto-jump race: opting out ("Let me browse instead") must
// cancel the pending timer, so it can never fire mid-navigation and fling the
// player into the scene they declined.
export const GenesisBrowseInstead: Story = {
  args: {
    arm: "genesis",
    jumpDelayMs: 150,
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "genesis",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ args, canvas }) => {
    await userEvent.click(await canvas.findByRole("link", { name: /browse instead/i }));
    // Wait well past jumpDelayMs; the auto-jump must NOT have navigated to the place.
    await new Promise((r) => setTimeout(r, 300));
    expect(args.navigate).not.toHaveBeenCalledWith(
      "/places/plc-genesis-plaza?from=open-screen",
    );
  },
};

// three-cards with no live reading: the two live-scene cards go disabled
// (non-interactive), but the chooser never hard dead-ends -- "Customize your
// avatar" stays a live link.
export const ThreeCardsNoLive: Story = {
  args: {
    arm: "three-cards",
    busiest: null,
    surprise: null,
    trackCtx: {
      sid: "sb-spec",
      story: "client/open-screen",
      variant: "three-cards",
      experimentKey: "client_open_screen",
    },
  },
  play: async ({ canvas }) => {
    await canvas.findByRole("heading", { name: "What do you feel like?" });
    const disabled = await canvas.findAllByText("No live scene reading right now");
    expect(disabled).toHaveLength(2);
    await canvas.findByText("Customize your avatar");
  },
};
