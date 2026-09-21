import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import { useState } from "react";
import Communities from "./Communities";
import type { SocialCommunity, SocialDetail } from "./Communities";
import { asset } from "../../asset";

const NOW = Date.parse("2026-09-21T12:00:00Z");

const COMMUNITIES: SocialCommunity[] = [
  { id: "builders", name: "Builders Guild", description: "Scene makers trading tips, assets and honest feedback on works in progress.", membersCount: 1284, privacy: "public", role: "owner", ownerName: "Nyx" },
  { id: "music", name: "Music Club", description: "Live sets every Friday, listening parties and a place to share what you are making.", membersCount: 642, privacy: "public", role: "member", isLive: true, ownerName: "vortex.eth" },
  { id: "racers", name: "Night Racers", description: "Time trials across Genesis City. Bring your fastest wearables.", membersCount: 311, privacy: "private", role: "moderator", ownerName: "korbin" },
  { id: "fashion", name: "Fashion District", description: "Wearable creators and collectors. Weekly runway, monthly drops.", membersCount: 2230, privacy: "public", role: "none", ownerName: "pixelwitch" },
  { id: "photo", name: "Photo Walks", description: "Slow tours of beautiful places, camera in hand.", membersCount: 188, privacy: "public", role: "none", ownerName: "Maple" },
  { id: "dao", name: "Governance Roundtable", description: "Proposals explained in plain words, before the vote closes.", membersCount: 957, privacy: "private", role: "none", ownerName: "Astra" },
];

const FRIENDS = [
  { name: "Nyx", online: true, status: "online" as const, where: "Genesis Plaza", hue: 280, address: "0x1111111111111111111111111111111111aaaa" },
  { name: "pixelwitch", online: true, status: "away" as const, where: "Away", hue: 320, address: "0x2222222222222222222222222222222222bbbb" },
  { name: "vortex.eth", online: true, status: "online" as const, where: "Vegas City", hue: 200, address: "0x3333333333333333333333333333333333cccc" },
  { name: "Maple", online: false, status: "offline" as const, where: "Offline", hue: 95, address: "0x4444444444444444444444444444444444dddd" },
  { name: "korbin", online: false, status: "offline" as const, where: "Offline", hue: 30, address: "0x5555555555555555555555555555555555eeee" },
];

const EVENTS = [
  { id: "ev-1", name: "Friday Live Set: vortex.eth", startAt: "2026-09-25T20:00:00Z", attendees: 214, live: false },
  { id: "ev-2", name: "Build Review Night", startAt: "2026-09-27T18:30:00Z", attendees: 58, live: false },
  { id: "ev-3", name: "Runway: Autumn Drop", startAt: "2026-10-02T19:00:00Z", attendees: 1320, live: false },
];

const NEARBY = [
  { address: "0x7777777777777777777777777777777777aaaa", name: "ghostrunner", coords: "0,0" },
  { address: "0x1111111111111111111111111111111111aaaa", name: "Nyx", coords: "1,0" },
  { address: "0x8888888888888888888888888888888888bbbb", name: "Astra", coords: "0,-1" },
];

const MEMBERS = [
  { memberAddress: "0x1111111111111111111111111111111111aaaa", name: "Nyx", role: "owner", hasClaimedName: true },
  { memberAddress: "0x5555555555555555555555555555555555eeee", name: "korbin", role: "moderator" },
  { memberAddress: "0x9999999999999999999999999999999999cccc", name: "You", role: "member" },
  { memberAddress: "0x7777777777777777777777777777777777aaaa", name: "ghostrunner", role: "member" },
  { memberAddress: "0x8888888888888888888888888888888888bbbb", name: "Astra", role: "member", hasClaimedName: true },
  { memberAddress: "0x6666666666666666666666666666666666ffff", name: "Lulu", role: "member" },
];

const POSTS = [
  { id: "p8", authorAddress: MEMBERS[0]!.memberAddress, authorName: "Nyx", authorHasClaimedName: true, content: "Doors open at eight tonight. The review stage has new lights, come and break them in.", createdAt: "2026-09-20T18:05:00Z", likesCount: 3 },
  { id: "p7", authorAddress: MEMBERS[1]!.memberAddress, authorName: "korbin", content: "Reminder: keep works in progress in the side rooms so the plaza stays walkable.", createdAt: "2026-09-19T11:20:00Z", likesCount: 5 },
  { id: "p4", authorAddress: MEMBERS[0]!.memberAddress, authorName: "Nyx", authorHasClaimedName: true, content: "Build Review Night is this Sunday. Bring one scene, leave with three ideas.", createdAt: "2026-09-18T17:40:00Z", likesCount: 12 },
  { id: "p3", authorAddress: MEMBERS[1]!.memberAddress, authorName: "korbin", content: "The shared asset shelf moved to the new plaza. Same door, more room.", createdAt: "2026-08-02T09:15:00Z", likesCount: 7 },
  { id: "p2", authorAddress: MEMBERS[0]!.memberAddress, authorName: "Nyx", authorHasClaimedName: true, content: "Thank you all for the spring jam. Forty scenes in one weekend.", createdAt: "2026-04-11T20:05:00Z", likesCount: 31 },
  { id: "p6", authorAddress: MEMBERS[1]!.memberAddress, authorName: "korbin", content: "Moderators now hold office hours on Wednesdays. Bring questions, bugs and half-finished ideas.", createdAt: "2025-11-03T16:45:00Z", likesCount: 18 },
  { id: "p5", authorAddress: MEMBERS[0]!.memberAddress, authorName: "Nyx", authorHasClaimedName: true, content: "One thousand members. Thank you for building with us.", createdAt: "2025-02-14T09:30:00Z", likesCount: 96 },
  { id: "p1", authorAddress: MEMBERS[0]!.memberAddress, authorName: "Nyx", authorHasClaimedName: true, content: "Welcome to the guild. Say hello in General and tell us what you are building.", createdAt: "2024-06-01T10:00:00Z", likesCount: 54 },
];

const PLACES = [
  { id: "plaza", title: "Builders Plaza", description: "Workbenches, the asset shelf and the review stage.", image: asset("assets/scene-thumb.png"), location: "-12,48", addedBy: MEMBERS[0]!.memberAddress, addedAt: "2026-09-02T12:00:00Z" },
  { id: "jam", title: "Jam Hall", description: "Where the weekend jams happen.", location: "buildersguild.dcl.eth", addedBy: MEMBERS[1]!.memberAddress, addedAt: "2025-05-20T12:00:00Z" },
];

function Social(props: ComponentProps<typeof Communities>) {
  const [id, setId] = useState<string | null>(props.initialCommunityId ?? null);
  const community = COMMUNITIES.find((c) => c.id === id) ?? null;
  const detail: SocialDetail | null = id ? { id, community, members: MEMBERS, posts: POSTS, places: PLACES } : null;
  return (
    <Communities
      communities={COMMUNITIES}
      friends={FRIENDS}
      events={EVENTS}
      nearby={NEARBY}
      selfAddress={MEMBERS[2]!.memberAddress}
      detail={detail}
      onSelect={setId}
      now={NOW}
      {...props}
    />
  );
}

const meta = {
  title: "Explorer/Pages/Communities",
  component: Communities,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Communities>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Social />,
};

export const Empty: Story = {
  render: () => <Communities />,
};

export const Explore: Story = {
  render: () => <Social initialSection="explore" />,
};

export const Friends: Story = {
  render: () => <Social initialSection="friends" />,
};

export const Nearby: Story = {
  render: () => <Social initialSection="nearby" />,
};

export const Summary: Story = {
  render: () => <Social initialCommunityId="builders" />,
};

export const General: Story = {
  render: () => <Social initialCommunityId="builders" initialChannel="general" />,
};

export const Announcements: Story = {
  render: () => <Social initialCommunityId="builders" initialChannel="announcements" />,
};

export const Hangouts: Story = {
  render: () => <Social initialCommunityId="builders" initialChannel="hangouts" />,
};

export const Members: Story = {
  render: () => <Social initialCommunityId="builders" initialChannel="members" />,
};
