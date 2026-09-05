import { MemoryRouter } from "react-router";

import type { Decorator, Meta, StoryObj } from "@ui/docs/sb";
import { expect, userEvent, waitFor } from "@ui/docs/sb";

import type { CommunityModerationCard } from "@data/lib/catalyst/admin/community-moderation";
import type { SuspendFn } from "./machine";
import ModerateCommunitiesWizard from "./ModerateCommunitiesWizard";

const card = (over: Partial<CommunityModerationCard>): CommunityModerationCard => ({
  id: "c-0",
  name: "Community",
  owner: "0x89aB3c00112233445566778899aabbccddeeff00",
  ownerName: null,
  privacy: "public",
  active: true,
  suspended: false,
  membersCount: 12,
  thumbnail: "",
  flaggedReason: "",
  status: "Active",
  hue: 280,
  ...over,
});

const cards: CommunityModerationCard[] = [
  card({
    id: "c-plaza",
    name: "Plaza Builders",
    ownerName: "plazaboss",
    membersCount: 148,
    flaggedReason: "spam invites",
    hue: 200,
  }),
  card({
    id: "c-neon",
    name: "Neon Racers",
    membersCount: 41,
    privacy: "private",
    hue: 320,
  }),
  card({
    id: "c-dust",
    name: "Dust Collective",
    suspended: true,
    active: false,
    status: "Suspended",
    membersCount: 9,
    hue: 40,
  }),
];

const suspend: SuspendFn = async ({ communityId, decision }) => ({
  ok: true,
  id: communityId,
  suspended: decision === "suspend",
});

const withRouter: Decorator = (Story) => (
  <MemoryRouter>
    <Story />
  </MemoryRouter>
);

const meta = {
  title: "Sites Specs/admin/communities-moderation/ModerateCommunitiesWizard",
  component: ModerateCommunitiesWizard,
  decorators: [withRouter],
  parameters: {
    layout: "padded",
    a11y: { test: "todo" },
  },
  args: {
    trackCtx: { sid: "sb-spec", story: "admin/communities-moderation", variant: "wizard" },
    cards,
    suspend,
    track: () => {},
  },
} satisfies Meta<typeof ModerateCommunitiesWizard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const AuthGate: Story = {};

export const List: Story = { args: { initialStep: "list" } };

export const ReviewCommunity: Story = { args: { initialStep: "review-community" } };

export const Decision: Story = { args: { initialStep: "decision" } };

export const Moderated: Story = { args: { initialStep: "moderated" } };

export const OpensList: Story = {
  play: async ({ canvas }) => {
    await userEvent.click(
      await canvas.findByRole("button", { name: "Continue to moderation list" }),
    );
    await canvas.findByText("Plaza Builders");
    await waitFor(() =>
      expect(
        canvas.queryByRole("button", { name: "Continue to moderation list" }),
      ).toBeNull(),
    );
  },
};
