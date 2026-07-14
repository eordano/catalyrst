import type { Meta, StoryObj } from "@storybook/react-vite";
import ChDataSourcesPage from "./ChDataSourcesPage";
import { FIXTURE_NOW, at } from "../lib/datum.fixtures";
import { sourceGroups, sourceGroupsAllDown } from "../lib/activity.fixtures";

const GROUPS = {
  all: sourceGroups,
  probesFailing: sourceGroupsAllDown,
  none: [],
};

const meta = {
  title: "CreatorHub/Pages/ChDataSourcesPage",
  component: ChDataSourcesPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    groups: { control: "select", options: Object.keys(GROUPS), mapping: GROUPS },
    filter: {
      control: "select",
      options: [
        "all",
        "live",
        "sampled",
        "snapshot",
        "unavailable",
        "unbuilt",
        "excluded",
      ],
    },
    now: { control: false },
    readAt: { control: "text" },
  },
  args: {
    groups: sourceGroups,
    readAt: at(0),
    filter: "all",
    now: FIXTURE_NOW,
    onFilterChange: () => {},
    onRefresh: () => {},
  },
} satisfies Meta<typeof ChDataSourcesPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Every probe failed. The page still renders -- and says so, per row. */
export const AllProbesFailing: Story = {
  args: { groups: GROUPS.probesFailing },
};

export const FilteredToUnbuilt: Story = { args: { filter: "unbuilt" } };

/** A filter with no matching rows says so rather than rendering a blank page. */
export const FilteredToNothing: Story = { args: { filter: "snapshot" } };

export const Refreshing: Story = { args: { refreshing: true } };

/** No registry at all -- an honest blank, never an invented row. */
export const EmptyRegistry: Story = { args: { groups: GROUPS.none } };

/** The read time is unknown and the page declines to guess one. */
export const NoReadTime: Story = { args: { readAt: null } };
