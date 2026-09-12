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
    await canvas.findByText("Jump into the action");
    await canvas.findByText("Surprise me");
    await canvas.findByText("Customize your avatar");
  },
};

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
    await new Promise((r) => setTimeout(r, 300));
    expect(args.navigate).not.toHaveBeenCalledWith(
      "/places/plc-genesis-plaza?from=open-screen",
    );
  },
};

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
