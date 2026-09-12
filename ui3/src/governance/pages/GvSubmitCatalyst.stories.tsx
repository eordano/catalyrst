import type { Meta, StoryObj } from "@storybook/react-vite";
import GvSubmitCatalyst from "./GvSubmitCatalyst";

const CATALYST_TYPES = ["add", "remove"] as const;
const STATES = ["form", "login", "notfound"] as const;

const meta = {
  title: "Governance/Pages/Submit Catalyst",
  component: GvSubmitCatalyst,
  parameters: { layout: "fullscreen" },
  argTypes: {
    catalystType: {
      control: "select",
      options: CATALYST_TYPES,
      description: "Which `COPY` block drives the title, description and field labels.",
    },
    state: {
      control: "select",
      options: STATES,
      description: "Which body the page mounts: the form, the sign-in gate, or the 404.",
    },
    showError: {
      control: "boolean",
      description: "Shows the form's validation error. Only visible while `state` is `form`.",
    },
  },
  args: { catalystType: "add", state: "form", showError: false },
} satisfies Meta<typeof GvSubmitCatalyst>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Remove: Story = { args: { catalystType: "remove" } };

export const LogInGate: Story = { args: { state: "login" } };

export const SubmitError: Story = { args: { showError: true } };

export const NotFound: Story = { args: { state: "notfound" } };
