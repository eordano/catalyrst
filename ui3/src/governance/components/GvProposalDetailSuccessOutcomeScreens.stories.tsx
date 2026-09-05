import type { ComponentProps, CSSProperties } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProposalDetailSuccessOutcomeScreens from "./GvProposalDetailSuccessOutcomeScreens";

const VARIANTS = ["new", "update", "pending", "bid"] as const;

/** Every outcome screen, in the order the variant stories used to declare them. */
const CASES: ComponentProps<typeof GvProposalDetailSuccessOutcomeScreens>[] = [
  { variant: "new" },
  { variant: "pending" },
  { variant: "bid" },
  { variant: "update" },
  { variant: "new", loading: true },
];

const meta = {
  title: "Governance/Components/Proposal-detail outcomes",
  component: GvProposalDetailSuccessOutcomeScreens,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: {
      control: "select",
      options: VARIANTS,
      description:
        "Which `VARIANTS` entry drives the copy. `pending` renders the pending card, the rest render the success card.",
    },
    loading: {
      control: "boolean",
      description: "Spinner on the dismiss action. Only the success card reads it.",
    },
  },
  args: { variant: "new", loading: false },
} satisfies Meta<typeof GvProposalDetailSuccessOutcomeScreens>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

// The modal backdrop is `position: fixed; inset: 0`, so stacked entries would all pile onto the
// viewport. `transform` makes each <section> the containing block for its own fixed backdrop.
const cell: CSSProperties = {
  position: "relative",
  height: 520,
  transform: "translateZ(0)",
};

/**
 * Every outcome rendered at once. `Default` flips between them with the Controls panel; this story
 * keeps all five in the render + a11y + visual-diff gates, since the success card, the pending card
 * and the loading state are structurally different subtrees.
 */
export const Catalog: Story = {
  name: "Catalog (every outcome)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
      {CASES.map((props, i) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={i} style={cell}>
          <GvProposalDetailSuccessOutcomeScreens {...props} />
        </section>
      ))}
    </div>
  ),
};
