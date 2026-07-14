import BlogPostRoute from "../routes/blog.$slug";
import { findBlogPost, relatedBlogPosts } from "@core/lib/content/blog";
import { routeStory } from "./lib";

const SLUG = "explore-from-your-browser";
const post = findBlogPost(SLUG);
if (!post) throw new Error(`blog-post story: post "${SLUG}" missing from app/lib/content/blog`);

export default {
  title: "Routes/BlogPost",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Post = {
  render: routeStory({
    Component: BlogPostRoute,
    path: "/blog/:slug",
    url: `/blog/${SLUG}`,
    loaderData: { slug: SLUG, post, related: relatedBlogPosts(post), sid: "story-sid" },
  }),
};

export const NotFound = {
  render: routeStory({
    Component: BlogPostRoute,
    path: "/blog/:slug",
    url: "/blog/no-such-post",
    loaderData: { slug: "no-such-post", post: null, related: [], sid: "story-sid" },
  }),
};
