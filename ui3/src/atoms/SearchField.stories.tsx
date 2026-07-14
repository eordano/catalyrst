import type { Meta, StoryObj } from "@storybook/react-vite";
import SearchField from "./SearchField";

const meta = {
  title: "Atoms/SearchField",
  component: SearchField,
  tags: ["autodocs"],
  parameters: { layout: "centered" },
  argTypes: {
    placeholder: { control: "text" },
    defaultValue: { control: "text" },
    value: { control: "text" },
  },
  args: { placeholder: "Search" },
} satisfies Meta<typeof SearchField>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
