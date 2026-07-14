import type { Meta, StoryObj } from "@storybook/react-vite";
import GvAccountIdentityLinkingFlow from "./GvAccountIdentityLinkingFlow";

/** Every `initial` view the flow's switch can start on, in flow order. */
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
  // The component latches `initial` into useState on mount, so without a key derived from it
  // the control would look dead: changing the arg would re-render the same instance, which
  // keeps its first view. The key forces a remount per value.
  render: ({ initial }) => <GvAccountIdentityLinkingFlow key={initial} initial={initial} />,
} satisfies Meta<typeof GvAccountIdentityLinkingFlow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every step rendered at once. `Default` flips between them with the `initial` control;
 * this story keeps all eight in the render + a11y + visual-diff gates, since each is a
 * structurally different view (choose list, connection steps, push spinner, post-connection
 * result, unlink confirmation dialog).
 */
export const Catalog: Story = {
  name: "Catalog (every step)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="gv" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {VIEWS.map((view) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={view}>
          <GvAccountIdentityLinkingFlow initial={view} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
