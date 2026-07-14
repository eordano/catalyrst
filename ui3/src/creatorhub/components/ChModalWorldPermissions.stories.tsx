import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChModalWorldPermissions from "./ChModalWorldPermissions";

const ACCESS_VIEWS = [
  "default",
  "invite_form",
  "password_form",
  "clear_list_confirm",
  "change_access_type_confirm",
];
const COLLABORATOR_VIEWS = ["default", "add", "clear_confirmation", "empty", "parcels"];

const meta = {
  title: "CreatorHub/Components/World Permissions",
  component: ChModalWorldPermissions,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    tab: { control: "inline-radio", options: ["access", "collaborators"] },
    view: {
      control: "select",
      options: [...new Set([...ACCESS_VIEWS, ...COLLABORATOR_VIEWS])],
      description: "Sub-view within the active tab. Access: " + ACCESS_VIEWS.join(", ") + ". Collaborators: " + COLLABORATOR_VIEWS.join(", ") + ".",
    },
    accessType: { control: "select", options: ["unrestricted", "allowList", "sharedSecret"] },
    inviteTab: { control: "inline-radio", options: ["wallet", "community", "csv"] },
    loading: { control: "boolean" },
  },
  args: {
    variant: "panel",
    tab: "access",
    view: "default",
    accessType: "allowList",
    inviteTab: "wallet",
    loading: false,
  },
} satisfies Meta<typeof ChModalWorldPermissions>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type WpProps = ComponentProps<typeof ChModalWorldPermissions>;

const CASES: { label: string; args: Partial<WpProps> }[] = [
  { label: "Access \u{B7} invitation only", args: { accessType: "allowList" } },
  { label: "Access \u{B7} public", args: { accessType: "unrestricted" } },
  { label: "Access \u{B7} password protected", args: { accessType: "sharedSecret" } },
  { label: "Invite \u{B7} wallet", args: { view: "invite_form", inviteTab: "wallet" } },
  { label: "Invite \u{B7} community", args: { view: "invite_form", inviteTab: "community" } },
  { label: "Invite \u{B7} CSV", args: { view: "invite_form", inviteTab: "csv" } },
  { label: "Password dialog", args: { view: "password_form", accessType: "sharedSecret" } },
  { label: "Clear allow-list confirm", args: { view: "clear_list_confirm" } },
  { label: "Change access type confirm", args: { view: "change_access_type_confirm" } },
  { label: "Collaborators", args: { tab: "collaborators" } },
  { label: "Collaborators \u{B7} empty", args: { tab: "collaborators", view: "empty" } },
  { label: "Collaborators \u{B7} add", args: { tab: "collaborators", view: "add" } },
  { label: "Collaborators \u{B7} clear confirm", args: { tab: "collaborators", view: "clear_confirmation" } },
  { label: "Collaborators \u{B7} parcels", args: { tab: "collaborators", view: "parcels" } },
  { label: "Loading", args: { loading: true } },
];

export const Catalog: Story = {
  name: "Catalog (every view)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChModalWorldPermissions {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
