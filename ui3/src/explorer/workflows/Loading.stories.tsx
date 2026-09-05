import type { Meta, StoryObj } from "@storybook/react-vite";
import Loading from "./Loading";

const meta = {
  title: "Explorer/Workflows/Loading",
  component: Loading,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Loading>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Loading />,
};

export const TakeAShot: Story = {
  render: () => <Loading initialTip={0} />,
};

export const HangOut: Story = {
  render: () => <Loading initialTip={9} />,
};

export const NearlyDone: Story = {
  render: () => <Loading progress={99} />,
};
