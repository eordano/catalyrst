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

export const Default: Story = {};

export const NoAddress: Story = { args: { address: null } };

export const WorldListUnavailable: Story = {
  args: { worlds: unavailableWorlds },
};

export const NoWorlds: Story = { args: { worlds: emptyWorlds } };

export const UpstreamsDegraded: Story = { args: { ...indexDatumsDegraded } };

export const ParcelLookup: Story = {
  args: { parcel: parcelActivity, parcelPointer: "-3,-2" },
};

export const ParcelWithNoHistory: Story = {
  args: { parcel: parcelNoHistory, parcelPointer: "88,-91" },
};

export const Refreshing: Story = { args: { refreshing: true } };
