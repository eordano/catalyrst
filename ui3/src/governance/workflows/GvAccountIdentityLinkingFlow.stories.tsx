import type { Meta, StoryObj } from "@storybook/react-vite";
import GvAccountIdentityLinkingFlow from "./GvAccountIdentityLinkingFlow";

const VIEWS = [
  "choose",
  "unlink-row",
  "forum",
  "discord",
  "push",
  "post-success",
  "post-error",
  "unlink-confirm",
] as const;

const meta = {
  title: "Governance/Workflows/Identity linking",
  component: GvAccountIdentityLinkingFlow,
  parameters: { layout: "fullscreen" },
  argTypes: {
    initial: {
      control: "select",
      options: VIEWS,
      description: "Which step of the linking flow the component mounts on.",
    },
  },
  args: { initial: "choose" },
  render: ({ initial }) => <GvAccountIdentityLinkingFlow key={initial} initial={initial} />,
} satisfies Meta<typeof GvAccountIdentityLinkingFlow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Catalog: Story = {
  name: "Catalog (every step)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="gv" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {VIEWS.map((view) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        <section key={view}>
          <GvAccountIdentityLinkingFlow initial={view} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
