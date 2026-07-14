import { MemoryRouter } from "react-router";

import type { Decorator, Meta, StoryObj } from "@ui/docs/sb";
import { expect, userEvent, waitFor } from "@ui/docs/sb";

import type { UserBan } from "@data/lib/catalyst/admin/user-bans";
import type { CommitFn } from "./machine";
import UserBanWizard from "./UserBanWizard";

const ban = (over: Partial<UserBan>): UserBan => ({
  id: "ban-0",
  bannedAddress: "0x89aB3c00112233445566778899aabbccddeeff00",
  bannedBy: "0x11ff000000000000000000000000000000000011",
  reason: "harassment in Genesis Plaza",
  customMessage: null,
  bannedDeviceId: null,
  bannedAt: "2026-06-28T09:30:00Z",
  expiresAt: null,
  liftedAt: null,
  liftedBy: null,
  createdAt: "2026-06-28T09:30:00Z",
  name: "griefer.dcl.eth",
  ...over,
});

const bans: UserBan[] = [
  ban({ id: "ban-101" }),
  ban({
    id: "ban-102",
    bannedAddress: "0x22aa000000000000000000000000000000000022",
    reason: "scam links in chat",
    expiresAt: "2026-08-01T00:00:00Z",
    name: "",
  }),
];

const commit: CommitFn = async ({ action, address }) => ({ action, address });

const withRouter: Decorator = (Story) => (
  <MemoryRouter>
    <Story />
  </MemoryRouter>
);

const meta = {
  title: "Sites Specs/admin/operator-user-bans/UserBanWizard",
  component: UserBanWizard,
  decorators: [withRouter],
  parameters: {
    layout: "fullscreen",
    a11y: { test: "todo" },
  },
  args: {
    trackCtx: { sid: "sb-spec", story: "admin/operator-user-bans", variant: "wizard" },
    bans,
    moderator: "0x11ff000000000000000000000000000000000011",
    commit,
    track: () => {},
  },
} satisfies Meta<typeof UserBanWizard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const AuthGate: Story = {};

export const Bans: Story = { args: { initialStep: "bans" } };

export const Action: Story = { args: { initialStep: "action" } };

export const Confirm: Story = { args: { initialStep: "confirm" } };

export const Done: Story = { args: { initialStep: "done" } };

export const OpensBanList: Story = {
  play: async ({ canvas }) => {
    await userEvent.click(
      await canvas.findByRole("button", { name: "Continue to moderation" }),
    );
    await waitFor(() =>
      expect(
        canvas.queryByRole("button", { name: "Continue to moderation" }),
      ).toBeNull(),
    );
  },
};
