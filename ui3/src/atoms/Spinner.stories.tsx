import type { Meta, StoryObj } from "@storybook/react-vite";
import Spinner from "./Spinner";

const meta = {
  title: "Atoms/Spinner",
  component: Spinner,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    size: { control: { type: "range", min: 12, max: 80, step: 2 } },
    color: { control: "color" },
  },
  args: { size: 28 },
} satisfies Meta<typeof Spinner>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
