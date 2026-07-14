import type { Meta, StoryObj } from "@storybook/react-vite";
import ChActivityIndexPage from "./ChActivityIndexPage";
import { FIXTURE_NOW, at } from "../lib/datum.fixtures";
import {
  emptyWorlds,
  indexDatums,
  indexDatumsDegraded,
  parcelActivity,
  parcelNoHistory,
  sourceGroups,
  unavailableWorlds,
} from "../lib/activity.fixtures";

const meta = {
  title: "CreatorHub/Pages/ChActivityIndexPage",
  component: ChActivityIndexPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    now: { control: false },
    address: { control: "text" },
  },
  args: {
    address: "0x313d\u{2026}9a1",
    readAt: at(0),
    ...indexDatums,
    sources: sourceGroups,
    now: FIXTURE_NOW,
    onRefresh: () => {},
    onConnect: () => {},
    onAddressSubmit: () => {},
    onPointerLookup: () => {},
  },
} satisfies Meta<typeof ChActivityIndexPage>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * The full page: a live world list, one sampled headcount, one real zero, one
 * "no sample" that is explicitly not a zero, one never-deployed NAME and one
 * blocked world.
 */
export const Default: Story = {};

/** No address yet. Scoping, not a sign-in wall -- and the copy says so. */
export const NoAddress: Story = { args: { address: null } };

/** The world list read failed. One panel naming the endpoint, not an empty table. */
export const WorldListUnavailable: Story = {
  args: { worlds: unavailableWorlds },
};

/** A real 200 with zero rows. That is an answer, and it gets an empty state. */
export const NoWorlds: Story = { args: { worlds: emptyWorlds } };

/** Presence and the worlds server are both down; the page degrades in place. */
export const UpstreamsDegraded: Story = { args: { ...indexDatumsDegraded } };

/** `?pointer=x,y` -- the working escape hatch beside the unbuilt parcel panel. */
export const ParcelLookup: Story = {
  args: { parcel: parcelActivity, parcelPointer: "-3,-2" },
};

/** The collector has never polled that pointer. Not a zero series. */
export const ParcelWithNoHistory: Story = {
  args: { parcel: parcelNoHistory, parcelPointer: "88,-91" },
};

export const Refreshing: Story = { args: { refreshing: true } };
