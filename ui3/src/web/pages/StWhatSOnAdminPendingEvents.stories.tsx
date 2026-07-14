import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import StWhatSOnAdminPendingEvents from "./StWhatSOnAdminPendingEvents";
import type { EventItem } from "./StWhatSOnAdminPendingEvents";

const PENDING: EventItem[] = [
  { id: "p1", name: "Synthwave Rooftop Sessions", creator: "soundscape.dcl", time: "21:00", dateLabel: "TODAY", hue: 264 },
  { id: "p2", name: "Vegas City High-Roller Night", creator: "VegasCityDAO", time: "20:30", dateLabel: "TOMORROW", hue: 268 },
  { id: "p3", name: "Builder Workshop: Smart Items 201", creator: "0x7c\u{2026}a4e1", time: "14:00", dateLabel: "IN 3 DAYS", hue: 130 },
  { id: "p4", name: "Wearable Drop: Neon Forge", creator: "neonforge.dcl", time: "19:00", dateLabel: "12 JUL", hue: 320 },
  { id: "p5", name: "Open Mic Trivia & Hangout", creator: "hangouts.dcl", time: "17:30", dateLabel: "14 JUL", hue: 96 },
];

const APPROVED: EventItem[] = [
  { id: "a1", name: "DAO Town Hall \u{2014} Q3 Treasury", creator: "governance.dcl", time: "16:00", dateLabel: "TODAY", hue: 30 },
  { id: "a2", name: "Metaverse Art Gallery Opening", creator: "cryptoart.dcl", time: "19:00", dateLabel: "TOMORROW", hue: 210 },
  { id: "a3", name: "Dragon City Night Market", creator: "dragoncity.dcl", time: "18:00", dateLabel: "13 JUL", hue: 18 },
];

/** Both review queues as one payload preset. Ignored while `loading` or `allowed: false`. */
const QUEUES = {
  full: { pending: PENDING, approved: APPROVED },
  empty: { pending: [], approved: [] },
} satisfies Record<string, { pending: EventItem[]; approved: EventItem[] }>;

type QueueKey = keyof typeof QUEUES;
const QUEUE_KEYS = Object.keys(QUEUES) as QueueKey[];

/** Story args: the queues are picked by name, `loading`/`allowed` pass straight through. */
type PendingEventsStoryArgs = Omit<
  ComponentProps<typeof StWhatSOnAdminPendingEvents>,
  "pending" | "approved"
> & { queue: QueueKey };

const meta = {
  title: "Web/Pages/What's On/Admin Pending Events",
  component: StWhatSOnAdminPendingEvents,
  parameters: { layout: "fullscreen" },
  argTypes: {
    queue: {
      control: "inline-radio",
      options: QUEUE_KEYS,
      description: "Which queue payload fills the two sections: cards, or both empty.",
    },
    loading: { control: "boolean", description: "Replaces the whole page body with the spinner." },
    allowed: {
      control: "boolean",
      description: "`false` replaces the body with the not-authorized notice.",
    },
  },
  args: { queue: "full", loading: false, allowed: true },
  render: ({ queue, ...rest }) => (
    <StWhatSOnAdminPendingEvents {...QUEUES[queue]} {...rest} />
  ),
} satisfies Meta<PendingEventsStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Both queues empty. */
export const Empty: Story = { args: { queue: "empty" } };

/** The spinner that replaces the whole body. */
export const Loading: Story = { args: { queue: "empty", loading: true } };

/** The not-authorized notice. */
export const Unauthorized: Story = { args: { queue: "empty", allowed: false } };
