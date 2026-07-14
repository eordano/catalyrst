import type { Meta, StoryObj } from "@storybook/react-vite";
import ErrorPage from "./ErrorPage";

const meta = {
  tags: ["autodocs"],
  title: "Components/ErrorPage",
  component: ErrorPage,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ErrorPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    title: "Oops!",
    message: "TypeError: Cannot read properties of undefined (reading 'profile')",
    detail:
      "TypeError: Cannot read properties of undefined (reading 'profile')\n    at ProfileCard (app/components/ProfileCard.tsx:42:18)\n    at renderWithHooks (react-dom)",
    isDev: false,
  },
};

export const Dev: Story = {
  args: { ...Default.args!, isDev: true },
};
