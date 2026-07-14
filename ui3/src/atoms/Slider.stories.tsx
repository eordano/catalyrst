import type { Meta, StoryObj } from "@storybook/react-vite";
import Slider from "./Slider";

const meta = {
  title: "Atoms/Slider",
  component: Slider,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    defaultValue: { control: { type: "range", min: 0, max: 100, step: 1 } },
    min: { control: "number" },
    max: { control: "number" },
    step: { control: "number" },
    label: { control: "text" },
    ariaLabel: { control: "text" },
    disabled: { control: "boolean" },
  },
  args: {
    defaultValue: 50,
    min: 0,
    max: 100,
    step: 1,
    ariaLabel: "Volume",
    disabled: false,
  },
} satisfies Meta<typeof Slider>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
