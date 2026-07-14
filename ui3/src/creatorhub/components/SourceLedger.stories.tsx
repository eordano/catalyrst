import type { Meta, StoryObj } from "@storybook/react-vite";
import SourceLedger from "./SourceLedger";
import { FIXTURE_NOW } from "../lib/datum.fixtures";
import {
  sourceGroups,
  sourceGroupsAllDown,
} from "../lib/activity.fixtures";

const GROUPS = {
  all: sourceGroups,
  probesFailing: sourceGroupsAllDown,
  onlyEmptySnapshot: sourceGroups.filter((g) => g.klass === "snapshot"),
  onlyExcluded: sourceGroups.filter((g) => g.klass === "excluded"),
};

const meta = {
  title: "CreatorHub/Components/SourceLedger",
  component: SourceLedger,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Every datum the hub can show, its endpoint, its probed state, and what reads it \u{2014} including the endpoints deliberately *not* displayed, with the reason. `live` and `sampled` rows carry a probe result; `unbuilt` and `excluded` rows are constants and are never probed.",
      },
    },
  },
  argTypes: {
    groups: {
      control: "select",
      options: Object.keys(GROUPS),
      mapping: GROUPS,
    },
    now: { control: false },
  },
  args: { groups: sourceGroups, now: FIXTURE_NOW },
  decorators: [
    (Story) => (
      <div style={{ padding: 24, maxWidth: 1180, margin: "0 auto" }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof SourceLedger>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** The probes ran and every host refused. The ledger says so per row. */
export const ProbesFailing: Story = {
  args: { groups: GROUPS.probesFailing },
};

/** A legitimately empty group still appears, with the reason it is empty. */
export const EmptySnapshotGroup: Story = {
  args: { groups: GROUPS.onlyEmptySnapshot },
};

/** Known-bad numbers, listed so nobody rediscovers them and wires them up. */
export const ExcludedOnPurpose: Story = {
  args: { groups: GROUPS.onlyExcluded },
};
