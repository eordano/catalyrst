import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import StBlogHome from "./StBlogHome";
import type { BlogPost } from "./StBlogHome";

const POSTS: BlogPost[] = [
  {
    id: "p1",
    title: "Decentraland 2.0 is Here: A New Era for the Open Metaverse",
    description:
      "The biggest update in Decentraland's history brings a brand-new desktop client, faster load times, and a redesigned in-world experience built from the ground up.",
    publishedDate: "JUN 18, 2026",
    category: { title: "Announcements", url: "/blog/announcements" },
    hue: 268,
  },
  {
    id: "p2",
    title: "Metaverse Fashion Week Returns Bigger Than Ever",
    description:
      "Top brands, independent designers, and wearable creators converge for a week of runway shows, drops, and after-parties across the city.",
    publishedDate: "JUN 14, 2026",
    category: { title: "Community", url: "/blog/community" },
    hue: 320,
  },
  {
    id: "p3",
    title: "How the DAO Voted to Fund the Next Wave of Creators",
    description: "Inside the latest treasury grant round and what it means for builders.",
    publishedDate: "JUN 10, 2026",
    category: { title: "Ecosystem", url: "/blog/ecosystem" },
    hue: 200,
  },
  {
    id: "p4",
    title: "Building Smart Items: A Beginner's Guide to the Creator Hub",
    description: "From your first scene to a published interactive experience.",
    publishedDate: "JUN 6, 2026",
    category: { title: "Tutorials", url: "/blog/tutorials" },
    hue: 130,
  },
  {
    id: "p5",
    title: "Genesis Plaza Gets a Makeover: What's New in the Welcome Hub",
    description: "A refreshed onboarding experience for every new explorer.",
    publishedDate: "JUN 2, 2026",
    category: { title: "Platform", url: "/blog/platform" },
    hue: 48,
  },
  {
    id: "p6",
    title: "Spotlight: The Communities Building the Future of the Metaverse",
    description: "Meet the DAOs, districts, and collectives shaping Decentraland.",
    publishedDate: "MAY 28, 2026",
    category: { title: "Community", url: "/blog/community" },
    hue: 305,
  },
  {
    id: "p7",
    title: "Wearable Drop: CryptoArt Studios Launches Limited Collection",
    description: "A new collaboration brings rare emotes and outfits to the marketplace.",
    publishedDate: "MAY 24, 2026",
    category: { title: "Ecosystem", url: "/blog/ecosystem" },
    hue: 18,
  },
];

/** The feed payload, picked by name: seven posts or nothing. */
const POST_SETS = { full: POSTS, empty: [] } satisfies Record<string, BlogPost[]>;
type PostSetKey = keyof typeof POST_SETS;
const POST_SET_KEYS = Object.keys(POST_SETS) as PostSetKey[];

/** Story args: the feed is picked by name, `loading`/`error` pass straight through. */
type BlogHomeStoryArgs = Omit<ComponentProps<typeof StBlogHome>, "posts"> & {
  postSet: PostSetKey;
};

const meta = {
  title: "Web/Pages/Blog/Home",
  component: StBlogHome,
  parameters: { layout: "fullscreen" },
  argTypes: {
    postSet: {
      control: "inline-radio",
      options: POST_SET_KEYS,
      description: "Which feed fills the page: seven posts, or none (empty state).",
    },
    loading: {
      control: "boolean",
      description: "With an empty feed this renders the initial skeleton.",
    },
    error: { control: "boolean", description: "Replaces the feed with the error state." },
  },
  args: { postSet: "full", loading: false, error: false },
  render: ({ postSet, ...rest }) => <StBlogHome posts={POST_SETS[postSet]} {...rest} />,
} satisfies Meta<BlogHomeStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/*
 * Reachable from the `postSet` control, but kept as its own export because the story id is a
 * hardcoded external consumer: tools/screen-tour/add-story-links.mts lists
 * `web-pages-blog-home--empty` in its MAPS and only console.warns when an id stops resolving,
 * so dropping the export would silently break that deep link.
 */
export const Empty: Story = { args: { postSet: "empty" } };

/** The initial skeleton, before the feed resolves. */
export const Loading: Story = { args: { postSet: "empty", loading: true } };

/** The feed-failed notice. */
export const Error: Story = { args: { postSet: "empty", error: true } };
