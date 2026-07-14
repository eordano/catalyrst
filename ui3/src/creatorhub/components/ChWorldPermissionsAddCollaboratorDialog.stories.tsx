import type { ReactElement } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChWorldPermissionsAddCollaboratorDialog from "./ChWorldPermissionsAddCollaboratorDialog";

/**
 * The component's props are inferred from destructuring defaults (`error = null`), so its
 * `error` prop types as `null` and no string is assignable to it. Re-declare the props the
 * stories drive and cast once here -- the story file cannot fix the component.
 */
type AcdStoryArgs = {
  variant?: "modal" | "panel";
  value?: string;
  error?: string | null;
  chrome?: boolean;
  onClose?: () => void;
};

const AddCollaboratorDialog = ChWorldPermissionsAddCollaboratorDialog as unknown as (
  props: AcdStoryArgs,
) => ReactElement;

const meta = {
  title: "CreatorHub/Components/World Permissions: Add Collaborator",
  component: AddCollaboratorDialog,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: { control: "select", options: ["modal", "panel"] },
    value: { control: "text", description: "Pre-filled wallet address." },
    error: { control: "text", description: "Validation error shown under the field." },
    chrome: {
      control: "boolean",
      description: "Off renders the bare form without the dialog shell.",
    },
  },
  args: { variant: "panel", value: "", error: null, chrome: true },
  // `value` / `error` seed internal state, so remount when a control changes them.
  render: (args) => <AddCollaboratorDialog key={`${args.value}|${args.error}`} {...args} />,
} satisfies Meta<AcdStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: AcdStoryArgs }[] = [
  { label: "Empty", args: { value: "", error: null } },
  { label: "Filled", args: { value: "0x4a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d", error: null } },
  { label: "Invalid address", args: { value: "0x123", error: "Invalid address" } },
  { label: "Bare form (no dialog chrome)", args: { value: "", error: null, chrome: false } },
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
          <AddCollaboratorDialog {...args} variant="panel" {...c.args} />
        </section>
      ))}
    </div>
  ),
};
