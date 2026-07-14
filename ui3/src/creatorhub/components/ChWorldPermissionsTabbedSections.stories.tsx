import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChWorldPermissionsTabbedSections from "./ChWorldPermissionsTabbedSections";

const WORLD_NAME = "myworld.dcl.eth";
const OWNER = "0x9f3c2b1a7d8e4c5f6a0b1c2d3e4f5a6b7c8d9e21";

const ACCESS_WALLETS = [
  { address: "0x2a8b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b", name: "DesignLead.dcl.eth", role: "collaborator" },
  { address: "0x71c0ffee00d34dbeefcafe1234567890abcdef12", name: "alice.eth" },
  { address: "0x4b22f3a91d0e8c7b6a5f4e3d2c1b0a9f8e7d6c5b" },
  { address: "0x88aa77bb66cc55dd44ee33ff2211009988776655", name: "bob.eth" },
];

const COLLABORATORS = [
  { address: "0x2a8b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b", name: "DesignLead.dcl.eth", deployment: "world-wide", parcelsCount: 0 },
  { address: "0x55de01ab23cd45ef67ab89cd01ef23ab45cd67ef", name: "builder.eth", deployment: "parcels", parcelsCount: 6 },
  { address: "0xc0ffee2548cafe9876543210fedcba0123456789", deployment: "none", parcelsCount: 0 },
];

const meta = {
  title: "CreatorHub/Components/World Permissions tabs",
  component: ChWorldPermissionsTabbedSections,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    initialTab: {
      control: "inline-radio",
      options: ["access", "collaborators", "parcels"],
      description: "`parcels` opens the Collaborators tab drilled into one collaborator's parcels.",
    },
    accessType: { control: "select", options: ["unrestricted", "allow-list", "shared-secret"] },
    open: { control: "boolean" },
    worldName: { control: "text" },
    ownerAddress: { control: "text" },
    ownerName: { control: "text" },
    accessWallets: { control: "object", description: "Empty array renders the empty allow-list." },
    collaborators: { control: "object", description: "Empty array renders the empty state." },
  },
  args: {
    variant: "panel",
    open: true,
    worldName: WORLD_NAME,
    ownerAddress: OWNER,
    accessWallets: ACCESS_WALLETS,
    collaborators: COLLABORATORS,
    initialTab: "access",
  },
  // `initialTab` seeds internal state, so remount when the control changes it.
  render: (args) => <ChWorldPermissionsTabbedSections key={args.initialTab} {...args} />,
} satisfies Meta<typeof ChWorldPermissionsTabbedSections>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type WpProps = ComponentProps<typeof ChWorldPermissionsTabbedSections>;

const CASES: { label: string; args: Partial<WpProps> }[] = [
  { label: "Access tab", args: { initialTab: "access" } },
  { label: "Collaborators tab", args: { initialTab: "collaborators" } },
  { label: "Collaborators \u{B7} parcels drilldown", args: { initialTab: "parcels" } },
  {
    label: "Collaborators \u{B7} empty",
    args: { initialTab: "collaborators", accessWallets: [], collaborators: [] },
  },
];

export const Catalog: Story = {
  name: "Catalog (every tab)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChWorldPermissionsTabbedSections {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
