import type { Meta, StoryObj } from "@storybook/react-vite";
import Friends from "./Friends";

const FRIENDS = [
  { name: "Nyx", tag: "#a91f", online: true, status: "online" as const, where: "Genesis Plaza", hue: 280, address: "0x1111111111111111111111111111111111aaaa", hasClaimedName: true },
  { name: "pixelwitch", tag: "#0c2d", online: true, status: "away" as const, where: "Soul Magic", hue: 320, address: "0x2222222222222222222222222222222222bbbb" },
  { name: "vortex.eth", tag: "#7e10", online: true, status: "online" as const, where: "Vegas City", hue: 200, address: "0x3333333333333333333333333333333333cccc", hasClaimedName: true },
  { name: "Maple", tag: "#33ab", online: false, where: "Offline", hue: 95, address: "0x4444444444444444444444444444444444dddd" },
  { name: "korbin", tag: "#5d41", online: false, where: "Offline", hue: 30, address: "0x5555555555555555555555555555555555eeee" },
  { name: "Lulu", tag: "#b2e9", online: false, where: "Offline", hue: 340, address: "0x6666666666666666666666666666666666ffff" },
];

const RECEIVED = [
  { name: "ghostrunner", tag: "#4f21", date: "JUN 24", hue: 150, address: "0x7777777777777777777777777777777777aaaa" },
  { name: "Astra", tag: "#9c70", date: "JUN 22", hue: 260, address: "0x8888888888888888888888888888888888bbbb", hasClaimedName: true },
];
const SENT = [{ name: "deckard", tag: "#1188", date: "JUN 20", hue: 210, address: "0x9999999999999999999999999999999999cccc" }];
const BLOCKED = [{ name: "spambot42", tag: "#0000", date: "JAN 12", hue: 0, address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaadddd" }];

const meta = {
  title: "Explorer/Pages/Friends",
  component: Friends,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Friends>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Friends friends={FRIENDS} received={RECEIVED} sent={SENT} blocked={BLOCKED} />
  ),
};

export const Guest: Story = {
  render: () => <Friends isGuest friends={FRIENDS} received={RECEIVED} sent={SENT} blocked={BLOCKED} />,
};

export const NoBlocked: Story = {
  render: () => (
    <Friends initialSection="blocked" friends={FRIENDS} received={RECEIVED} sent={SENT} blocked={[]} />
  ),
};
