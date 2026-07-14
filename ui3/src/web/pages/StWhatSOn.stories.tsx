import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import StWhatSOn from "./StWhatSOn";
import type { WoDayEvent, WoLiveCard, WoUpcomingCard } from "./StWhatSOn";

const LIVE_NOW: WoLiveCard[] = [
  { id: "ln1", title: "Vegas City Casino Night", users: 312, isEvent: true, creator: "VegasCityDAO", hue: 268 },
  { id: "ln2", title: "Genesis Plaza Welcome Hub", users: 526, isEvent: false, creator: "Decentraland", hue: 200 },
];

const UPCOMING: WoUpcomingCard[] = [
  { id: "up1", name: "Metaverse Fashion Week Runway", creator: "MVFW", time: "Today 18:00", hue: 320 },
  { id: "up2", name: "DAO Town Hall \u{2014} Q2 Treasury", creator: "governance.dcl", time: "Tomorrow 16:00", hue: 30 },
  { id: "up3", name: "Music Festival: Synthwave Stage", creator: "soundscape.dcl", time: "Starts in 3 hours", hue: 264 },
  { id: "up4", name: "Builder Workshop: Smart Items 101", creator: "0x7c\u{2026}a4e1", time: "Fri 14:00", hue: 130 },
  { id: "up5", name: "Wearable Drop: CryptoArt Studios", creator: "cryptoart.dcl", time: "Sat 20:00", hue: 48 },
  { id: "up6", name: "Casino Poker Championship", creator: "VegasCityDAO", time: "Sun 21:00", hue: 0 },
  { id: "up7", name: "Art Gallery Opening Night", creator: "0xab\u{2026}77d3", time: "Mon 19:00", hue: 210 },
  { id: "up8", name: "Trivia & Hangout \u{2014} Open Mic", creator: "hangouts.dcl", time: "Tue 17:30", hue: 96 },
];

const DAY_LABELS: string[] = ["Today", "Tomorrow", "Wed", "Thu", "Fri"];
const ALL_DAYS: WoDayEvent[][] = [
  [
    { id: "a1", name: "Casino Night", creator: "VegasCityDAO", time: "18:00", live: true, x: 72, y: 12, users: 312, hue: 268 },
    { id: "a2", name: "Synthwave DJ Set", creator: "soundscape.dcl", time: "21:00", live: false, hue: 264 },
    { id: "a3", name: "Open Mic Trivia", creator: "hangouts.dcl", time: "22:30", live: false, hue: 96 },
  ],
  [
    { id: "b1", name: "DAO Town Hall", creator: "governance.dcl", time: "16:00", live: false, hue: 30 },
    { id: "b2", name: "Fashion Runway", creator: "MVFW", time: "18:00", live: false, hue: 320 },
  ],
  [
    { id: "c1", name: "Smart Items 101", creator: "0x7c\u{2026}a4e1", time: "14:00", live: false, hue: 130 },
  ],
  [
    { id: "d1", name: "Gallery Opening", creator: "0xab\u{2026}77d3", time: "19:00", live: false, hue: 210 },
    { id: "d2", name: "Poker Championship", creator: "VegasCityDAO", time: "21:00", live: false, hue: 0 },
  ],
  [],
];

type Feed = {
  liveNow: WoLiveCard[];
  upcoming: WoUpcomingCard[];
  allDays: WoDayEvent[][];
  dayLabels: string[];
};

/** A whole page payload per preset: the live rail, the upcoming rail and the day calendar. */
const FEEDS = {
  full: { liveNow: LIVE_NOW, upcoming: UPCOMING, allDays: ALL_DAYS, dayLabels: DAY_LABELS },
  noLiveNow: { liveNow: [], upcoming: UPCOMING, allDays: ALL_DAYS, dayLabels: DAY_LABELS },
  emptyCalendar: { liveNow: [], upcoming: [], allDays: [[], [], [], [], []], dayLabels: [] },
} satisfies Record<string, Feed>;

type FeedKey = keyof typeof FEEDS;
const FEED_KEYS = Object.keys(FEEDS) as FeedKey[];

/** Story args: the page payload is picked by name, `loading` is passed straight through. */
type WhatsOnStoryArgs = Omit<
  ComponentProps<typeof StWhatSOn>,
  "liveNow" | "upcoming" | "allDays" | "dayLabels"
> & { feed: FeedKey };

const meta = {
  title: "Web/Pages/What's On/Discovery",
  component: StWhatSOn,
  parameters: { layout: "fullscreen" },
  argTypes: {
    feed: {
      control: "select",
      options: FEED_KEYS,
      description:
        "Which payload preset is rendered: everything, an empty Live Now rail, or an empty calendar.",
    },
    loading: {
      control: "boolean",
      description: "Defers the Upcoming and All Experiences sections; only Live Now renders.",
    },
  },
  args: { feed: "full", loading: false },
  render: ({ feed, ...rest }) => <StWhatSOn {...FEEDS[feed]} {...rest} />,
} satisfies Meta<WhatsOnStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Upcoming and All Experiences deferred; only Live Now renders. */
export const Loading: Story = { args: { loading: true } };

/** An empty Live Now rail. */
export const NoLiveNow: Story = { args: { feed: "noLiveNow" } };

/** An empty calendar. */
export const EmptyCalendar: Story = { args: { feed: "emptyCalendar" } };
