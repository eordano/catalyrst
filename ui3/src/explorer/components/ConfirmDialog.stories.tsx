import type { Meta, StoryObj } from "@storybook/react-vite";
import ConfirmDialog from "./ConfirmDialog";

const meta = {
  title: "Explorer/Components/ConfirmDialog",
  component: ConfirmDialog,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ConfirmDialog>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    title: "Are you sure?",
    body: "This action will remove the item from your backpack.",
    confirmLabel: "Yes",
    cancelLabel: "No",
  },
};

export const Danger: Story = {
  args: {
    title: "Delete forever?",
    body: "This cannot be undone.",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
    danger: true,
  },
};

export const GradientPrimary: Story = {
  args: {
    title: "Equip wearable?",
    body: "This will replace your current outfit slot.",
    confirmLabel: "Confirm",
    cancelLabel: "Cancel",
    variant: "gradient",
    gradient: "purple",
    confirmTone: "primary",
  },
};
