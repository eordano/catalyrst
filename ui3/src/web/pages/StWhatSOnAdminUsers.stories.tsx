import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import StWhatSOnAdminUsers from "./StWhatSOnAdminUsers";
import type { UserRow } from "./StWhatSOnAdminUsers";

const USERS: UserRow[] = [
  { user: "0x3c1f8a92b4e6d70f5a9c2e18b3d47e60a1f29c8d", name: "vegascitydao", permissions: ["approve_own_event", "approve_any_event", "edit_any_event", "edit_any_profile"], hue: 268 },
  { user: "0x7e4b21d9f0a3c65e8b1d72f04a6c98e3b5d710a2", name: "governance.dcl", permissions: ["approve_any_event", "edit_any_event"], hue: 30 },
  { user: "0xab09f3e2c7d145896b0e4a2f81c63d70e5a9b218", name: "metaversefw", permissions: ["approve_own_event"], hue: 320 },
  { user: "0x12c8b7a04e6f93d2510a8c7e3b94f06d2a8e15c4", name: null, permissions: ["approve_any_event", "edit_any_event", "edit_any_profile"], hue: 200 },
  { user: "0x5f9a3d80c2e147b6e90a4f31d8b27c05a6e93f1b", name: "soundscape.dcl", permissions: ["approve_own_event", "approve_any_event"], hue: 264 },
  { user: "0x90e7c4a13b6d28f5019e3a7c41d80b62f5a9e034", name: "dragoncity.dcl", permissions: ["edit_any_event"], hue: 18 },
  { user: "0x2d6b18f9a04e7c35b1f8d20a96c43e07d5a8b921", name: null, permissions: ["approve_own_event"], hue: 130 },
  { user: "0xc41a8e07b39d62f5104e8a2c7b95f306d1a9e842", name: "cryptoart.dcl", permissions: ["approve_any_event", "edit_any_event", "edit_any_profile"], hue: 48 },
  { user: "0x6b39d02a8c1e745f90b3e2a6c84d17f05e9a3b60", name: "hangouts.dcl", permissions: ["approve_own_event", "approve_any_event", "edit_any_event"], hue: 96 },
  { user: "0x83f0a91c5d2e647b8f10a3c7e29d45b06f1a8e93", name: null, permissions: ["edit_any_profile"], hue: 210 },
  { user: "0x1a7c93e02b8d465f9013a6c2e74f80d5b9a3e168", name: "wonderzone.dcl", permissions: ["approve_own_event"], hue: 305 },
  { user: "0x47e2b08d9a3c165f0b8e4a1c7d92f306e5a9b740", name: "builders.dcl", permissions: ["approve_any_event", "edit_any_event"], hue: 160 },
];

/** The table payload, picked by name: a full page of rows or nothing at all. */
const USER_SETS = { full: USERS, empty: [] } satisfies Record<string, UserRow[]>;
type UserSetKey = keyof typeof USER_SETS;
const USER_SET_KEYS = Object.keys(USER_SETS) as UserSetKey[];

/** `none` mounts with no banner; the other two are the two `severity` values. */
const FEEDBACK = {
  none: null,
  success: { message: "Permissions updated", severity: "success" },
  error: { message: "Unable to save permissions. Please try again.", severity: "error" },
} satisfies Record<string, { message: string; severity: string } | null>;
type FeedbackKey = keyof typeof FEEDBACK;
const FEEDBACK_KEYS = Object.keys(FEEDBACK) as FeedbackKey[];

/** Story args: both the rows and the initial feedback banner are picked by preset name. */
type AdminUsersStoryArgs = Omit<
  ComponentProps<typeof StWhatSOnAdminUsers>,
  "users" | "initialFeedback"
> & { userSet: UserSetKey; feedback: FeedbackKey };

const meta = {
  title: "Web/Pages/What's On/Admin Users",
  component: StWhatSOnAdminUsers,
  parameters: { layout: "fullscreen" },
  argTypes: {
    userSet: {
      control: "inline-radio",
      options: USER_SET_KEYS,
      description: "Which row set fills the table: twelve users, or none (empty state).",
    },
    feedback: {
      control: "select",
      options: FEEDBACK_KEYS,
      description: "Which `initialFeedback` banner the page mounts with.",
    },
  },
  args: { userSet: "full", feedback: "none" },
  // The page latches `initialFeedback` into useState on mount, so the `feedback` control would
  // look dead without a key derived from it: changing the arg re-renders the same instance, which
  // keeps its first banner. The key forces a remount per preset.
  render: ({ userSet, feedback, ...rest }) => (
    <StWhatSOnAdminUsers
      key={`${userSet}-${feedback}`}
      users={USER_SETS[userSet]}
      initialFeedback={FEEDBACK[feedback]}
      {...rest}
    />
  ),
} satisfies Meta<AdminUsersStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { key: string; userSet: UserSetKey; feedback: FeedbackKey }[] = [
  { key: "full", userSet: "full", feedback: "none" },
  { key: "empty", userSet: "empty", feedback: "none" },
  { key: "success", userSet: "full", feedback: "success" },
  { key: "error", userSet: "full", feedback: "error" },
];

/**
 * Every row-set/feedback combination rendered at once. `Default` flips between them with the
 * `userSet` and `feedback` controls; this story keeps all four in the render + a11y + visual-diff
 * gates, since between them they are the only coverage of the empty table and of both feedback
 * banner severities.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="st ui2" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {CATALOG.map(({ key, userSet, feedback }) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={key}>
          <StWhatSOnAdminUsers users={USER_SETS[userSet]} initialFeedback={FEEDBACK[feedback]} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
