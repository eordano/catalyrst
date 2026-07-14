import type { Meta, StoryObj } from "@storybook/react-vite";
import StCastNotFound from "./StCastNotFound";

const meta = {
  title: "Web/Pages/Cast/Not Found",
  component: StCastNotFound,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StCastNotFound>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <StCastNotFound />,
};

export const ShortCopy: Story = {
  render: () => (
    <StCastNotFound
      title="404 - Page Not Found"
      description="This Cast 2.0 stream doesn't exist."
    />
  ),
};
