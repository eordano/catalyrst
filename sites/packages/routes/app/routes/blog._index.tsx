import { useEffect, useRef } from "react";

import StBlogHome from "@ui/web/pages/StBlogHome";
import { blogCategories } from "@ui/data/blogCategories";
import "@ui/web/pages/stbloghome.css";

import { blogPostCards, type BlogPostCard } from "@core/lib/content/blog";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, trackExposure } from "@core/lib/telemetry/track";

import type { Route } from "./+types/blog._index";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "misc/blog";

export const handle = { agentMarkdown: "blogIndex" } satisfies AgentMarkdownHandle;

const FALLBACK: Assignment = {
  variant: "index_grid",
  flags: { mainPostHero: true },
  experimentKey: "lp_blog_index",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });
  const category = new URL(request.url).searchParams.get("category") ?? "";
  const posts = blogPostCards(category);
  const payload = { sid, posts, category };
  return wrap(payload);
}

type LoaderData = { sid: string; posts: BlogPostCard[]; category: string };

export default function BlogIndexRoute({ loaderData }: Route.ComponentProps) {
  const { sid, posts, category } = loaderData as LoaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("lp_blog_viewed", { post_count: posts.length }, { sid, story: STORY });
  }, [sid, posts.length]);

  function onPostClick(e: React.MouseEvent<HTMLDivElement>) {
    const anchor = (e.target as HTMLElement).closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    const m = /^\/blog\/([^/]+)$/.exec(href);
    if (!m) return;
    track("lp_blog_post_clicked", { slug: m[1] }, { sid, story: STORY });
  }

  return (
    <div className="blog-index-route" onClickCapture={onPostClick}>
      <StBlogHome posts={posts} categories={blogCategories(category)} />
    </div>
  );
}
