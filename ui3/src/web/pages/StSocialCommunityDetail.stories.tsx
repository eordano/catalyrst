import type { FC } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StSocialCommunityDetailRaw from "./StSocialCommunityDetail";

type CommunityDetailProps = {
  community?: {
    id: string;
    name: string;
    description: string;
    ownerAddress: string;
    ownerName: string;
    ownerProfilePicture: string;
    privacy: string;
    membersCount: number;
    thumbnail: string;
    role?: string;
  };
  members?: {
    memberAddress: string;
    name: string;
    role: string;
    hasClaimedName: boolean;
  }[];
  events?: {
    id: string;
    name: string;
    image: string;
    creatorName: string;
    timeLabel: string;
  }[];
  membersTotal?: number;
  isLoggedIn?: boolean;
  isMember?: boolean;
  hasPendingRequest?: boolean;
  isLoadingMembers?: boolean;
  isLoadingEvents?: boolean;
  state?: string;
  mobile?: boolean;
  chrome?: boolean;
};

const StSocialCommunityDetail =
  StSocialCommunityDetailRaw as unknown as FC<CommunityDetailProps>;

const COMMUNITY: NonNullable<CommunityDetailProps["community"]> = {
  id: "bafkreicommunity",
  name: "Vroom Vroom Racing Club",
  description:
    "A home for builders and racers in Decentraland. We host weekly grand-prix nights on community tracks, share tuning setups, and run a friendly ladder. New drivers always welcome \u{2014} grab a kart and say hi.",
  ownerAddress: "0x9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b",
  ownerName: "TurboNomad",
  ownerProfilePicture: "",
  privacy: "public",
  membersCount: 1842,
  thumbnail: "",
  role: "member",
};

const PRIVATE_COMMUNITY: NonNullable<CommunityDetailProps["community"]> = {
  id: "bafkreiprivate",
  name: "Founders Lounge",
  description: "An invite-only space for early Decentraland builders.",
  ownerAddress: "0x1111222233334444555566667777888899990000",
  ownerName: "GenesisDAO",
  ownerProfilePicture: "",
  privacy: "private",
  membersCount: 312,
  thumbnail: "",
};

const MEMBERS: NonNullable<CommunityDetailProps["members"]> = [
  { memberAddress: "0xa1", name: "TurboNomad", role: "owner", hasClaimedName: true },
  { memberAddress: "0xb2", name: "Pixel Drift", role: "moderator", hasClaimedName: true },
  { memberAddress: "0xc3", name: "NeonApex", role: "member", hasClaimedName: false },
  { memberAddress: "0xd4", name: "GridLockGail", role: "member", hasClaimedName: true },
  { memberAddress: "0xe5", name: "skidmark.eth", role: "member", hasClaimedName: false },
  { memberAddress: "0xf6", name: "Velvet Racer", role: "member", hasClaimedName: false },
  { memberAddress: "0xa7", name: "Chicane", role: "member", hasClaimedName: true },
];

const EVENTS: NonNullable<CommunityDetailProps["events"]> = [
  {
    id: "evt-1",
    name: "Friday Night Grand Prix",
    image: "",
    creatorName: "TurboNomad",
    timeLabel: "Starts in 2 hours",
  },
  {
    id: "evt-2",
    name: "Beginner Kart Clinic & Track Tour",
    image: "",
    creatorName: "Pixel Drift",
    timeLabel: "Tomorrow, 6:00 PM",
  },
  {
    id: "evt-3",
    name: "Community Track Showcase",
    image: "",
    creatorName: "NeonApex",
    timeLabel: "Sat, 8:00 PM",
  },
  {
    id: "evt-4",
    name: "Ladder Finals Watch Party",
    image: "",
    creatorName: "GridLockGail",
    timeLabel: "Sun, 4:00 PM",
  },
];

const COMMUNITIES = { racing: COMMUNITY, privateLounge: PRIVATE_COMMUNITY, none: undefined };
type CommunityKey = keyof typeof COMMUNITIES;

const MEMBER_SETS = { full: MEMBERS, empty: [] };
type MembersKey = keyof typeof MEMBER_SETS;

const EVENT_SETS = { full: EVENTS, empty: [] };
type EventsKey = keyof typeof EVENT_SETS;

const STATES = ["default", "loading", "notFound"];

/**
 * Story args: the three data blobs are picked by name. The preset keys deliberately do not
 * reuse the real prop names -- `Meta<TArgs>` type-checks `component:` as `ComponentType<TArgs>`,
 * so `TArgs` has to stay assignable to the component's own props.
 */
type CommunityStoryArgs = Omit<CommunityDetailProps, "community" | "members" | "events"> & {
  communityPreset: CommunityKey;
  memberSet: MembersKey;
  eventSet: EventsKey;
};

const meta = {
  title: "Web/Pages/Social/Community Detail",
  component: StSocialCommunityDetail,
  parameters: { layout: "fullscreen" },
  argTypes: {
    communityPreset: {
      control: "select",
      options: Object.keys(COMMUNITIES),
      description: "Which community descriptor is rendered; `none` falls back to the empty one.",
    },
    memberSet: { control: "inline-radio", options: Object.keys(MEMBER_SETS) },
    eventSet: { control: "inline-radio", options: Object.keys(EVENT_SETS) },
    state: {
      control: "inline-radio",
      options: STATES,
      description: "`loading` shows the spinner, `notFound` the missing-community screen.",
    },
    membersTotal: { control: "number" },
    isLoggedIn: { control: "boolean" },
    isMember: { control: "boolean" },
    hasPendingRequest: { control: "boolean" },
    isLoadingMembers: { control: "boolean" },
    isLoadingEvents: { control: "boolean" },
    mobile: { control: "boolean" },
  },
  args: {
    communityPreset: "racing",
    memberSet: "full",
    eventSet: "full",
    state: "default",
    isLoggedIn: false,
    isMember: false,
    mobile: false,
  },
  render: ({ communityPreset, memberSet, eventSet, ...rest }) => (
    <StSocialCommunityDetail
      community={COMMUNITIES[communityPreset]}
      members={MEMBER_SETS[memberSet]}
      events={EVENT_SETS[eventSet]}
      {...rest}
    />
  ),
} satisfies Meta<CommunityStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Signed in and already a member -- the leave CTA. */
export const SignedInMember: Story = { args: { isLoggedIn: true, isMember: true } };

/** A private community seen by a non-member -- the gate. */
export const PrivateGated: Story = {
  args: { communityPreset: "privateLounge", isLoggedIn: true },
};

/** No members and no events. */
export const Empty: Story = { args: { memberSet: "empty", eventSet: "empty" } };

/** The mobile tab switcher. */
export const MobileTabbed: Story = { args: { mobile: true } };

/** The loading spinner. */
export const Loading: Story = { args: { state: "loading" } };

/** The missing-community screen. */
export const NotFound: Story = { args: { state: "notFound" } };
