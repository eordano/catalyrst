import type { Meta, StoryObj } from "@storybook/react-vite";
import GvSubmitPitch from "./GvSubmitPitch";

const ACCOUNT = "0x9f3c\u{2026}7a21";

const SUBMIT_ERROR =
  "Error: proposal submission failed \u{2014} the governance service returned 500 (Internal Server Error).";

const meta = {
  title: "Governance/Pages/Submit Pitch",
  component: GvSubmitPitch,
  parameters: { layout: "fullscreen" },
  argTypes: {
    account: {
      control: "text",
      description: "Connected wallet. Empty renders the sign-in gate instead of the form.",
    },
    loading: {
      control: "boolean",
      description: "Renders the loading gate; checked before `account`, so it wins over the gate.",
    },
    vpNotMet: {
      control: "boolean",
      description: "Disables the form and shows the voting-power notice.",
    },
    error: { control: "text", description: "Submission error banner copy." },
  },
  args: { account: ACCOUNT, loading: false, vpNotMet: false, error: "" },
} satisfies Meta<typeof GvSubmitPitch>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** The voting-power notice, with the form disabled. */
export const VpNotMet: Story = { args: { vpNotMet: true } };

/** The submission error banner. */
export const Error: Story = { args: { error: SUBMIT_ERROR } };

/** The loading gate, checked before the account gate. */
export const Loading: Story = { args: { loading: true } };

/** The sign-in gate for a disconnected wallet. */
export const LoginGate: Story = { args: { account: "" } };
