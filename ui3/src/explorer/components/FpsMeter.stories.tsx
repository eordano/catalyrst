import type { Meta, StoryObj } from "@storybook/react-vite";
import FpsMeter from "./FpsMeter";

const meta = {
  title: "Explorer/Components/FpsMeter",
  component: FpsMeter,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof FpsMeter>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Healthy: Story = {
  render: () => <FpsMeter stats={{ page: 60, engine: 60, ms: 16.7 }} />,
};

export const HudStarvingTheFrame: Story = {
  render: () => <FpsMeter stats={{ page: 34, engine: 59, ms: 29.4 }} />,
};

export const Bad: Story = {
  render: () => <FpsMeter stats={{ page: 12, engine: 14, ms: 83.3 }} />,
};

export const NoEngine: Story = {
  render: () => <FpsMeter stats={{ page: 60, engine: null, ms: 16.7 }} />,
};
