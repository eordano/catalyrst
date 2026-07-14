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

/** The remove-catalyst copy block over the same form. */
export const Remove: Story = { args: { catalystType: "remove" } };

/** The sign-in gate that replaces the form for a disconnected wallet. */
export const LogInGate: Story = { args: { state: "login" } };

/** The form's validation error banner. */
export const SubmitError: Story = { args: { showError: true } };

/** The 404 body. */
export const NotFound: Story = { args: { state: "notfound" } };
