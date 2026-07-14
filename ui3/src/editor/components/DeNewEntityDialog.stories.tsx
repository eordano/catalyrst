import type { Meta, StoryObj } from "@storybook/react-vite";
import DeNewEntityDialog from "./DeNewEntityDialog";

const meta = {
  title: "Editor/Components/New Entity Dialog",
  component: DeNewEntityDialog,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DeNewEntityDialog>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <DeNewEntityDialog onCancel={() => {}} onCreate={() => {}} onPickParent={() => {}} />
  ),
};
