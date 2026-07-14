import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChWorldPermissionsSetChangePasswordDialo from "./ChWorldPermissionsSetChangePasswordDialo";

const meta = {
  title: "CreatorHub/Components/World Permissions: Password",
  component: ChWorldPermissionsSetChangePasswordDialo,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    isChanging: { control: "boolean", description: "Change-password copy instead of set-password." },
    initialPassword: {
      control: "text",
      description: "Under 8 chars / fewer than 2 digits shows the requirements error.",
    },
    initialConfirm: {
      control: "text",
      description: "Differing from the password shows the mismatch error.",
    },
  },
  args: { variant: "panel", isChanging: false, initialPassword: "", initialConfirm: "" },
  // The `initial*` props seed internal state, so remount when a control changes them.
  render: (args) => (
    <ChWorldPermissionsSetChangePasswordDialo
      key={`${args.isChanging}|${args.initialPassword}|${args.initialConfirm}`}
      {...args}
    />
  ),
} satisfies Meta<typeof ChWorldPermissionsSetChangePasswordDialo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type PwProps = ComponentProps<typeof ChWorldPermissionsSetChangePasswordDialo>;

const CASES: { label: string; args: Partial<PwProps> }[] = [
  { label: "Set password", args: { isChanging: false } },
  { label: "Change password", args: { isChanging: true } },
  { label: "Requirements not met", args: { isChanging: false, initialPassword: "abc1" } },
  {
    label: "Confirmation mismatch",
    args: { isChanging: false, initialPassword: "secret123", initialConfirm: "secret124" },
  },
  {
    label: "Valid",
    args: { isChanging: true, initialPassword: "secret123", initialConfirm: "secret123" },
  },
];

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChWorldPermissionsSetChangePasswordDialo {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
