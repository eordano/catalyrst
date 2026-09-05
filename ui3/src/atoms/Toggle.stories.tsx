import type { Meta, StoryObj } from "@storybook/react-vite";
import Toggle from "./Toggle";

const meta = {
  title: "Atoms/Toggle",
  component: Toggle,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    defaultChecked: { control: "boolean" },
    checked: { control: "boolean" },
    ariaLabel: { control: "text" },
    onChange: { action: "change" },
  },
  args: { defaultChecked: false, ariaLabel: "Toggle" },
} satisfies Meta<typeof Toggle>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
