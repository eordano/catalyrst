import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import StBlogPost from "./StBlogPost";

const SAMPLE_POST: NonNullable<ComponentProps<typeof StBlogPost>["post"]> = {
  id: "sample-post",
  slug: "sample-post",
  title: "Sample Post Title for the Blog Detail Layout",
  description:
    "A short description used to preview the blog post detail layout in Storybook.",
  publishedDate: "Jun 18, 2026",
  image: { url: "", width: 1200, height: 600 },
  category: { id: "announcements", title: "Announcements", slug: "announcements", url: "/blog?category=announcements" },
  author: {
    id: "sample-author",
    title: "Sample Author",
    slug: "sample-author",
    image: { url: "" },
    url: "/blog",
  },
  body: [
    { type: "p", content: "Opening paragraph of the sample post body." },
    { type: "h2", content: "A Section Heading" },
    { type: "p", content: "Another paragraph to show body rhythm and spacing." },
    { type: "quote", content: "A pull quote to preview the blockquote style." },
    { type: "h3", content: "A Subsection" },
    { type: "ul", items: ["First list item", "Second list item", "Third list item"] },
  ],
};

const SAMPLE_RELATED: NonNullable<ComponentProps<typeof StBlogPost>["related"]> = [
  {
    id: "rel-1",
    title: "Related Post One",
    publishedDate: "Jun 12, 2026",
    image: { url: "" },
    category: { title: "Platform", slug: "platform", url: "/blog?category=platform" },
    url: "/blog",
  },
  {
    id: "rel-2",
    title: "Related Post Two",
    publishedDate: "Jun 05, 2026",
    image: { url: "" },
    category: { title: "Community", slug: "community", url: "/blog?category=community" },
    url: "/blog",
  },
  {
    id: "rel-3",
    title: "Related Post Three",
    publishedDate: "May 29, 2026",
    image: { url: "" },
    category: { title: "Announcements", slug: "announcements", url: "/blog?category=announcements" },
    url: "/blog",
  },
];

/** `none` leaves `post` undefined, which is the page's own "nothing to render" state. */
const POSTS = { sample: SAMPLE_POST, none: undefined } satisfies Record<
  string,
  ComponentProps<typeof StBlogPost>["post"]
>;
type PostKey = keyof typeof POSTS;
const POST_KEYS = Object.keys(POSTS) as PostKey[];

/** `none` passes `[]`, which hides the "Related posts" rail. */
const RELATED = { three: SAMPLE_RELATED, none: [] } satisfies Record<
  string,
  NonNullable<ComponentProps<typeof StBlogPost>["related"]>
>;
type RelatedKey = keyof typeof RELATED;
const RELATED_KEYS = Object.keys(RELATED) as RelatedKey[];

const STATES = ["ready", "loading", "error"] as const;

/** Story args: post and related rail are picked by preset name; `state` passes through. */
type BlogPostStoryArgs = Omit<ComponentProps<typeof StBlogPost>, "post" | "related"> & {
  postFixture: PostKey;
  relatedFixture: RelatedKey;
};

const meta = {
  title: "Web/Pages/Blog/Post",
  component: StBlogPost,
  parameters: { layout: "fullscreen" },
  argTypes: {
    postFixture: {
      control: "inline-radio",
      options: POST_KEYS,
      description: "Which post is rendered; `none` leaves the detail column empty.",
    },
    relatedFixture: {
      control: "inline-radio",
      options: RELATED_KEYS,
      description: "Three related cards, or none (the rail disappears).",
    },
    state: {
      control: "inline-radio",
      options: STATES,
      description: "`loading` and `error` replace the detail column with the spinner / notice.",
    },
  },
  args: { postFixture: "sample", relatedFixture: "three", state: "ready" },
  render: ({ postFixture, relatedFixture, ...rest }) => (
    <StBlogPost post={POSTS[postFixture]} related={RELATED[relatedFixture]} {...rest} />
  ),
} satisfies Meta<BlogPostStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** The spinner, before the post resolves. */
export const Loading: Story = {
  args: { postFixture: "none", relatedFixture: "none", state: "loading" },
};

/** The post-failed notice. */
export const Error: Story = {
  args: { postFixture: "none", relatedFixture: "none", state: "error" },
};

/** No related posts, so the rail disappears. */
export const NoRelated: Story = { args: { relatedFixture: "none" } };

/** No post at all -- the detail column is empty. */
export const Empty: Story = { args: { postFixture: "none", relatedFixture: "none" } };
