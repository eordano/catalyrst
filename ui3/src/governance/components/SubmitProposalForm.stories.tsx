import type { ComponentProps, ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import SubmitProposalForm from "./SubmitProposalForm";
import GovernanceChrome from "../frames/GovernanceChrome";
import { GOVERNANCE_FORMS } from "../../data/governanceForms";

type SubmitProposalFormProps = ComponentProps<typeof SubmitProposalForm>;
type FormKey = keyof typeof GOVERNANCE_FORMS;

const FORM_KEYS = Object.keys(GOVERNANCE_FORMS) as FormKey[];

const descriptor = (key: FormKey) => GOVERNANCE_FORMS[key] as SubmitProposalFormProps;

type FormStoryArgs = Omit<SubmitProposalFormProps, "title" | "subtitle" | "description" | "sections"> & {
  form: FormKey;
};

const wrap = (node: ReactNode) => <GovernanceChrome active="proposals">{node}</GovernanceChrome>;

const meta = {
  title: "Governance/Components/SubmitProposalForm",
  component: SubmitProposalForm,
  parameters: { layout: "fullscreen" },
  argTypes: {
    form: {
      control: "select",
      options: FORM_KEYS,
      description: "Which descriptor from `GOVERNANCE_FORMS` drives the fields.",
    },
    numbered: { control: "boolean" },
    disabled: { control: "boolean" },
    submitDisabled: { control: "boolean" },
    showBack: { control: "boolean" },
    submitLabel: { control: "text" },
    secondaryLabel: { control: "text" },
    vpNotice: { control: "text" },
    error: { control: "text" },
    errorLabel: { control: "text" },
    errorCollapsible: { control: "boolean" },
  },
  args: { form: "poll" },
  render: ({ form, ...rest }) => wrap(<SubmitProposalForm {...descriptor(form)} {...rest} />),
} satisfies Meta<FormStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const DisabledVpNotMet: Story = {
  args: {
    form: "poll",
    disabled: true,
    vpNotice:
      "You don't meet the Voting Power requirement to submit this poll. You need at least 100 VP.",
  },
};

export const ErrorCollapsible: Story = {
  args: {
    form: "governance",
    error: "500 \u{2014} createProposal failed: gateway timeout. Please try again later.",
    errorCollapsible: true,
  },
};

export const Grant: Story = { args: { form: "grant" } };

const CATALOG_EXTRAS: Partial<Record<FormKey, Partial<SubmitProposalFormProps>>> = {
  banName: { error: "Name is already banned" },
};

export const Catalog: Story = {
  name: "Catalog (every proposal type)",
  parameters: { controls: { disable: true } },
  render: () =>
    wrap(
      <div style={{ display: "flex", flexDirection: "column", gap: 48 }}>
        {FORM_KEYS.map((key) => (
          // <section> demotes each form's unnamed <aside> from `complementary` to `generic`
          <section key={key}>
            <SubmitProposalForm {...descriptor(key)} {...CATALOG_EXTRAS[key]} />
          </section>
        ))}
      </div>
    ),
};
