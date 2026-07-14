import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ChPublishWizardDeployProgressResult from "./ChPublishWizardDeployProgressResult";

type DeployProps = ComponentProps<typeof ChPublishWizardDeployProgressResult>;

const noop = () => {};

/**
 * Story args: the three "what happens next" callbacks are a single synthetic switch, since a
 * Controls panel cannot supply functions and their presence is what renders the Jump In /
 * Copy / Keep editing affordances.
 */
type DeployStoryArgs = Omit<DeployProps, "onJumpIn" | "onCopy" | "onKeepEditing"> & {
  withActions: boolean;
};

const withActionProps = (on: boolean): Partial<DeployProps> =>
  on ? { onJumpIn: noop, onCopy: noop, onKeepEditing: noop } : {};

const meta = {
  title: "CreatorHub/Workflows/Publish: Deploy",
  component: ChPublishWizardDeployProgressResult,
  parameters: { layout: "fullscreen" },
  argTypes: {
    state: {
      control: "select",
      options: ["idle", "exceeded", "deploying", "finishing", "complete", "error"],
    },
    isWorld: { control: "boolean" },
    url: { control: "text" },
    withActions: {
      control: "boolean",
      description: "Wire onJumpIn / onCopy / onKeepEditing (adds the post-deploy actions).",
    },
    inline: { control: "boolean" },
    sample: { control: "boolean" },
  },
  args: { state: "idle", isWorld: false, url: "", withActions: false },
  render: ({ withActions, ...rest }) => (
    <ChPublishWizardDeployProgressResult {...rest} {...withActionProps(withActions)} />
  ),
} satisfies Meta<DeployStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<DeployStoryArgs> }[] = [
  { label: "Idle", args: { state: "idle" } },
  { label: "Max file size exceeded", args: { state: "exceeded" } },
  { label: "Deploying", args: { state: "deploying" } },
  { label: "Finishing", args: { state: "finishing" } },
  {
    label: "Complete (World)",
    args: {
      state: "complete",
      isWorld: true,
      url: "https://decentraland.org/jump/?realm=neon-market.dcl.eth",
      withActions: true,
    },
  },
  { label: "Error", args: { state: "error" } },
];

export const Catalog: Story = {
  name: "Catalog (every deploy state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 40, padding: 24 }}>
      {CASES.map((c) => {
        const { withActions, ...rest } = { ...args, ...c.args };
        return (
          <section key={c.label} aria-label={c.label}>
            <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
              {c.label}
            </div>
            <ChPublishWizardDeployProgressResult {...rest} {...withActionProps(withActions)} />
          </section>
        );
      })}
    </div>
  ),
};
