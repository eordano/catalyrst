import type { Meta, StoryObj } from "@storybook/react-vite";
import DatumTile from "./DatumTile";
import {
  DATUM_FIXTURE_KEYS,
  FIXTURE_NOW,
  datumFixtures,
} from "../lib/datum.fixtures";

const COMPARE = {
  none: undefined,
  liveData: { label: "Comms room (/live-data)", datum: datumFixtures.live },
};

const meta = {
  title: "CreatorHub/Components/DatumTile",
  component: DatumTile,
  parameters: {
    docs: {
      description: {
        component:
          "The only way a number reaches a card. There is no `value` prop and no `unavailable` boolean \u{2014} a caller cannot pass a figure without also passing where it came from. The tile never moves or resizes when its state changes: it swaps its badge, and the value becomes `\u{2014}` at the same size and weight.",
      },
    },
  },
  argTypes: {
    datum: {
      control: "select",
      options: DATUM_FIXTURE_KEYS,
      mapping: datumFixtures,
    },
    compare: {
      control: "inline-radio",
      options: Object.keys(COMPARE),
      mapping: COMPARE,
      description:
        "A second host's measurement of the same thing. It renders only when both readings exist AND disagree.",
    },
    format: { control: false },
    now: { control: false },
  },
  args: {
    label: "In this world",
    datum: datumFixtures.sampled,
    now: FIXTURE_NOW,
    format: (v: number | string) =>
      typeof v === "number" ? v.toLocaleString("en-US") : v,
  },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 340 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof DatumTile>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Live: Story = {
  args: {
    datum: datumFixtures.live,
    label: "People in your worlds right now",
  },
};
export const Sampled: Story = { args: { datum: datumFixtures.sampled } };
export const Stale: Story = { args: { datum: datumFixtures.stale } };
export const Snapshot: Story = {
  args: {
    datum: datumFixtures.snapshot,
    label: "Snapshots with someone in it",
  },
};

/** A real zero is a real reading -- and it must say so, or it reads as absent. */
export const RealZero: Story = {
  args: {
    datum: datumFixtures.realZero,
    note: "a real zero \u{2014} sampled 2m ago, nobody in",
  },
};

export const NoSample: Story = { args: { datum: datumFixtures.noSample } };

export const Unavailable: Story = {
  args: {
    datum: datumFixtures.unavailable,
    label: "People in your worlds right now",
  },
};

export const Unbuilt: Story = {
  args: { datum: datumFixtures.unbuilt, label: "Unique visitors" },
};

/** Two hosts, two honest answers. Neither is picked; neither is averaged. */
export const Disagreeing: Story = {
  args: { datum: datumFixtures.sampled, compare: COMPARE.liveData },
};
