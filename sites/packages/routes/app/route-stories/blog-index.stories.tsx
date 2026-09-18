import BlogIndexRoute from "../routes/blog._index";
import { blogPostCards } from "@core/lib/content/blog";
import { routeStory } from "./lib";

const shop = { shopArm: "base", shopVariant: "base", shopExperimentKey: "lp_blog_shop_entry", railItems: null };

function blogStubLoader({ request }: { request: Request }) {
  const category = new URL(request.url).searchParams.get("category") ?? "";
  return { ...shop, sid: "story-sid", posts: blogPostCards(category), category };
}

export default {
  title: "Routes/BlogIndex",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const AllPosts = {
  render: routeStory({
    Component: BlogIndexRoute,
    path: "/blog",
    loaderData: { ...shop, sid: "story-sid", posts: blogPostCards(), category: "" },
    loader: blogStubLoader,
  }),
};

export const CategoryAnnouncements = {
  render: routeStory({
    Component: BlogIndexRoute,
    path: "/blog",
    url: "/blog?category=announcements",
    loaderData: { ...shop, sid: "story-sid", posts: blogPostCards("announcements"), category: "announcements" },
    loader: blogStubLoader,
  }),
};

export const EmptyCategory = {
  render: routeStory({
    Component: BlogIndexRoute,
    path: "/blog",
    url: "/blog?category=tutorials",
    loaderData: { ...shop, sid: "story-sid", posts: blogPostCards("tutorials"), category: "tutorials" },
    loader: blogStubLoader,
  }),
};
