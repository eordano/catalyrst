import type { Meta, StoryObj } from "@storybook/react-vite";
import StReportSuccess from "./StReportSuccess";

const meta = {
  title: "Web/Pages/Report/Success",
  component: StReportSuccess,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StReportSuccess>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
