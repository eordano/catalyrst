import type { Meta, StoryObj } from "@storybook/react-vite";
import DatumNote from "./DatumNote";
import {
  DATUM_FIXTURE_KEYS,
  FIXTURE_NOW,
  datumFixtures,
} from "../lib/datum.fixtures";

const meta = {
  title: "CreatorHub/Components/DatumNote",
  component: DatumNote,
  parameters: {
    docs: {
      description: {
        component:
          "The endpoint line plus the reason sentence. Mandatory beneath any tile in a `no-sample`, `unavailable` or `unbuilt` state \u{2014} the reason a value is missing is not optional chrome. Every sentence is derived from the datum, so the copy cannot drift between screens.",
      },
    },
  },
  argTypes: {
    datum: {
      control: "select",
      options: DATUM_FIXTURE_KEYS,
      mapping: datumFixtures,
    },
    now: { control: false },
  },
  args: { datum: datumFixtures.unavailable, now: FIXTURE_NOW },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 520 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof DatumNote>;

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
