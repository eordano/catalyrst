import type { Meta, StoryObj } from "@storybook/react-vite";
import ChWorldActivityPage from "./ChWorldActivityPage";
import {
  DEFAULT_CADENCE_SECONDS,
  live,
  sampled,
  unavailable,
} from "../lib/datum";
import { FIXTURE_NOW, at } from "../lib/datum.fixtures";
import {
  CATALYST,
  accessFacts,
  emptyWorldHistory,
  kvStorageUnavailable,
  permissionsCli,
  receptionFacts,
  sourceGroups,
  storageReading,
  unavailableWorldHistory,
  worldHistory,
  worldMeta,
  worldNotBuilt,
} from "../lib/activity.fixtures";

const READ_AT = at(0);
const TAKEN_AT = at(2 * 60_000);

const meta = {
  title: "CreatorHub/Pages/ChWorldActivityPage",
  component: ChWorldActivityPage,
  parameters: { layout: "fullscreen" },
  argTypes: { now: { control: false } },
  args: {
    world: "petbarn.dcl.eth",
    worldMeta,
    jumpUrl: "https://decentraland.org/jump/?realm=petbarn.dcl.eth",
    readAt: READ_AT,

    inThisWorld: sampled(
      2,
      `GET ${CATALYST}/presence/current/worlds`,
      TAKEN_AT,
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),
    commsRoom: live(
      3,
      "GET worlds-content-server.decentraland.org/live-data",
      READ_AT,
    ),
    realm: live("healthy, accepting users", `GET ${CATALYST}/about`, READ_AT),

    history: worldHistory,
    peak: sampled(
      6,
      `GET ${CATALYST}/presence/worlds/history?world=petbarn.dcl.eth&limit=5000`,
      TAKEN_AT,
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),
    occupiedSnapshots: sampled(
      "38 of 1 214",
      `GET ${CATALYST}/presence/worlds/history?world=petbarn.dcl.eth&limit=5000`,
      TAKEN_AT,
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),
    historyBegins: sampled(
      "2026-07-13 02:15Z",
      `GET ${CATALYST}/presence/worlds/history?world=petbarn.dcl.eth&limit=5000`,
      TAKEN_AT,
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),

    sceneUrn: live(
      "urn:decentraland:entity:bafkrei\u{2026}q4a",
      `GET ${CATALYST}/world/petbarn.dcl.eth/about`,
      READ_AT,
    ),
    spawnCoordinates: live(
      "0,0",
      `GET ${CATALYST}/world/petbarn.dcl.eth/about`,
      READ_AT,
    ),
    storage: storageReading,
    sceneKvStorage: kvStorageUnavailable,

    access: accessFacts,
    permissionsCli,
    permissionsHref:
      "/creator-hub/world-permissions?world=petbarn.dcl.eth&from=activity",

    reception: receptionFacts,
    notBuilt: worldNotBuilt,
    sources: sourceGroups,
    now: FIXTURE_NOW,
    onRefresh: () => {},
    onRetryHistory: () => {},
  },
} satisfies Meta<typeof ChWorldActivityPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const SourcesAgree: Story = {
  args: {
    commsRoom: live(
      2,
      "GET worlds-content-server.decentraland.org/live-data",
      READ_AT,
    ),
  },
};

export const PresenceDown: Story = {
  args: {
    inThisWorld: unavailable(
      `GET ${CATALYST}/presence/current/worlds`,
      null,
      `GET ${CATALYST}/presence/current/worlds did not respond.`,
    ),
    history: unavailableWorldHistory,
    peak: unavailable(
      `GET ${CATALYST}/presence/worlds/history`,
      null,
      `GET ${CATALYST}/presence/worlds/history did not respond.`,
    ),
    occupiedSnapshots: unavailable(
      `GET ${CATALYST}/presence/worlds/history`,
      null,
      `GET ${CATALYST}/presence/worlds/history did not respond.`,
    ),
    historyBegins: unavailable(
      `GET ${CATALYST}/presence/worlds/history`,
      null,
      `GET ${CATALYST}/presence/worlds/history did not respond.`,
    ),
  },
};

export const NoOccupancyRecorded: Story = {
  args: { history: emptyWorldHistory },
};

export const StaleSample: Story = {
  args: {
    inThisWorld: sampled(
      2,
      `GET ${CATALYST}/presence/current/worlds`,
      at(22 * 60_000),
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),
  },
};

export const EmptyRightNow: Story = {
  args: {
    inThisWorld: sampled(
      0,
      `GET ${CATALYST}/presence/current/worlds`,
      TAKEN_AT,
      DEFAULT_CADENCE_SECONDS,
      READ_AT,
    ),
    commsRoom: live(
      0,
      "GET worlds-content-server.decentraland.org/live-data",
      READ_AT,
    ),
  },
};

export const HeaderDegraded: Story = {
  args: {
    worldMeta: unavailable(
      "GET worlds-content-server.decentraland.org/worlds?authorized_deployer=",
      502,
      "GET worlds-content-server.decentraland.org/worlds?authorized_deployer= returned 502.",
    ),
  },
};

export const NotYourWorld: Story = { args: { deployedByCaller: false } };

export const NotFound: Story = {
  args: { world: "ghost.dcl.eth", notFound: true },
};
