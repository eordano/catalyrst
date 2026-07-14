import { useEffect, useRef } from "react";

import StBlogPost from "@ui/web/pages/StBlogPost";
import SitesChrome from "@ui/web/frames/SitesChrome";
import "@ui/web/pages/stblogpost.css";

import { findBlogPost, relatedBlogPosts } from "@core/lib/content/blog";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/blog.$slug";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "misc/blog";

export const handle = { agentMarkdown: "blogPost" } satisfies AgentMarkdownHandle;

type RelatedCard = ReturnType<typeof relatedBlogPosts>[number];

const FALLBACK: Assignment = {
  variant: "index_grid",
  flags: { mainPostHero: true },
  experimentKey: "lp_blog_index",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const slug = params.slug ?? "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const post = findBlogPost(slug);
  const related = post ? relatedBlogPosts(post) : [];

  const payload = { slug, post, related, sid };
  return wrap(payload, { status: post ? 200 : 404 });
}

export default function BlogPostRoute({ loaderData }: Route.ComponentProps) {
  const { slug, post, related, sid } = loaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    if (post) {
      track("lp_blog_post_viewed", { slug: post.slug }, { sid, story: STORY });
    }
  }, [post, sid]);

  if (!post) {
    return (
      <SitesChrome>
        <div style={EMPTY_WRAP}>
          <h1 style={EMPTY_TITLE}>Post not found</h1>
          <p style={EMPTY_SUB}>
            We couldn&apos;t find a blog post at &ldquo;{slug}&rdquo;.
          </p>
          <a href="/blog" style={EMPTY_LINK}>
            &#x2190; Back to the Blog
          </a>
        </div>
      </SitesChrome>
    );
  }

  return <StBlogPost post={post} related={related} state="ready" />;
}

const EMPTY_WRAP: React.CSSProperties = {
  maxWidth: 640,
  margin: "0 auto",
  padding: "120px 24px",
  color: "#fff",
  textAlign: "center",
};
const EMPTY_TITLE: React.CSSProperties = {
  fontSize: 32,
  fontWeight: 800,
  margin: "0 0 12px",
};
const EMPTY_SUB: React.CSSProperties = {
  fontSize: 16,
  color: "rgba(255,255,255,.7)",
  margin: "0 0 24px",
};
const EMPTY_LINK: React.CSSProperties = {
  color: "#ff2d55",
  fontWeight: 600,
  textDecoration: "none",
};
