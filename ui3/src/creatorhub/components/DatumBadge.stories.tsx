import type { Meta, StoryObj } from "@storybook/react-vite";
import DatumBadge, { DatumTally } from "./DatumBadge";
import { tallyStates } from "../lib/datum";
import {
  DATUM_FIXTURE_KEYS,
  FIXTURE_NOW,
  datumFixtures,
} from "../lib/datum.fixtures";

const meta = {
  title: "CreatorHub/Components/DatumBadge",
  component: DatumBadge,
  parameters: {
    docs: {
      description: {
        component:
          "The only thing in the tree that renders a state word. Glyph plus word, always \u{2014} colour never carries the meaning alone, so the state survives greyscale and a screen reader hears the word.",
      },
    },
  },
  argTypes: {
    datum: {
      control: "select",
      options: DATUM_FIXTURE_KEYS,
      mapping: datumFixtures,
      description: "Which provenance state the badge is describing.",
    },
    now: { control: false },
  },
  args: { datum: datumFixtures.live, now: FIXTURE_NOW },
} satisfies Meta<typeof DatumBadge>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Live: Story = { args: { datum: datumFixtures.live } };
export const Sampled: Story = { args: { datum: datumFixtures.sampled } };
export const Stale: Story = { args: { datum: datumFixtures.stale } };
export const Snapshot: Story = { args: { datum: datumFixtures.snapshot } };
export const NoSample: Story = { args: { datum: datumFixtures.noSample } };
export const Unavailable: Story = {
  args: { datum: datumFixtures.unavailable },
};
export const Unbuilt: Story = { args: { datum: datumFixtures.unbuilt } };

/** The header strip, which doubles as the legend for every "--" on a screen. */
export const Tally: Story = {
  render: () => (
    <DatumTally
      tally={tallyStates(
        [
          datumFixtures.live,
          datumFixtures.sampled,
          datumFixtures.stale,
          datumFixtures.unavailable,
          datumFixtures.unbuilt,
        ],
        FIXTURE_NOW,
      )}
    />
  ),
};
