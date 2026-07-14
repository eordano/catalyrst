export type BlogBlock =
  | { type: "p"; content: string }
  | { type: "h2"; content: string }
  | { type: "h3"; content: string }
  | { type: "h4"; content: string }
  | { type: "quote"; content: string }
  | { type: "ul"; items: string[] };

export type BlogCategory = { id: string; title: string; slug: string; url: string };

export type BlogPostCard = {
  id: string;
  slug: string;
  title: string;
  description: string;
  publishedDate: string;
  category: { title: string; url: string };
  hue: number;
};

export type BlogPost = {
  id: string;
  slug: string;
  title: string;
  description: string;
  publishedDate: string;
  image: { url: string; width: number; height: number };
  category: BlogCategory;
  author: {
    id: string;
    title: string;
    slug: string;
    image: { url: string };
    url: string;
  };
  hue: number;
  body: BlogBlock[];
};

const DCL_ONE = {
  id: "dcl-one",
  title: "catalyst.example.com",
  slug: "dcl-one",
  image: { url: "" },
  url: "/blog?author=dcl-one",
};

function cat(slug: string, title: string): BlogCategory {
  return { id: slug, slug, title, url: `/blog?category=${slug}` };
}

export const BLOG_POSTS: BlogPost[] = [
  {
    id: "explore-from-your-browser",
    slug: "explore-from-your-browser",
    title: "Explore Decentraland From Your Browser at catalyst.example.com/play",
    description:
      "The web explorer is live: jump into the world directly from a browser tab \u{2014} no download or install required.",
    publishedDate: "JUL 2, 2026",
    image: { url: "", width: 1200, height: 600 },
    category: cat("platform", "Platform"),
    author: DCL_ONE,
    hue: 268,
    body: [
      {
        type: "p",
        content:
          "You can now walk into Decentraland straight from a browser tab. The web explorer at catalyst.example.com/play loads the world without any download or install \u{2014} open the page, pick a spot, and you're in.",
      },
      { type: "h3", content: "What you need" },
      {
        type: "ul",
        items: [
          "A modern desktop browser with WebGPU support (recent Chrome works well)",
          "Nothing else \u{2014} no installer, no launcher",
        ],
      },
      {
        type: "p",
        content:
          "The web explorer connects to the same content network as the rest of catalyst.example.com, so the scenes, avatars, and events you see are the real thing. Head to catalyst.example.com/play and have a look around.",
      },
    ],
  },
  {
    id: "whats-on-live-events-guide",
    slug: "whats-on-live-events-guide",
    title: "What's On: A Live Guide to Events Across the World",
    description:
      "One page that shows what's happening in-world right now and what's coming up, with quick filters for today, this week, and recurring events.",
    publishedDate: "JUL 1, 2026",
    image: { url: "", width: 1200, height: 600 },
    category: cat("platform", "Platform"),
    author: DCL_ONE,
    hue: 200,
    body: [
      {
        type: "p",
        content:
          "Finding something to do in an open world shouldn't take detective work. The What's On page pulls live and upcoming events from the events index into a single view: what's happening right now at the top, and everything coming up grouped by day below.",
      },
      { type: "h3", content: "How to use it" },
      {
        type: "ul",
        items: [
          "Filter by Today, This week, or Recurring to narrow the list",
          "Search events by name",
          "Open any event for details and a link to jump straight to its location in-world",
        ],
      },
      {
        type: "p",
        content:
          "The listing reflects the event data this node serves \u{2014} no editorial curation, just what's actually scheduled. Check it out at /whats-on.",
      },
    ],
  },
  {
    id: "marketplace-credits-season-one",
    slug: "marketplace-credits-season-one",
    title: "Marketplace Credits Season One Is Live",
    description:
      "Complete weekly goals in the Marketplace to earn credits you can spend on wearables and emotes. Season One runs June 29 through August 24, 2026.",
    publishedDate: "JUN 29, 2026",
    image: { url: "", width: 1200, height: 600 },
    category: cat("announcements", "Announcements"),
    author: DCL_ONE,
    hue: 130,
    body: [
      {
        type: "p",
        content:
          "Season One of the Marketplace Credits program starts today. Each week you get a fresh set of goals; complete them, claim your credits, and spend them on wearables and emotes in the Marketplace.",
      },
      { type: "h3", content: "How the season works" },
      {
        type: "ul",
        items: [
          "Season One runs from June 29 to August 24, 2026 \u{2014} eight weekly rounds",
          "Goals reset every week; unclaimed progress doesn't carry over",
          "The season's total credit pool is capped at 100,000 MANA",
        ],
      },
      {
        type: "p",
        content:
          "Your progress, claimable credits, and the weekly countdown are all on the credits page in the Marketplace. Credits earned during the season expire when the season ends, so spend them before August 24.",
      },
    ],
  },
];

export function blogPostCards(categorySlug?: string): BlogPostCard[] {
  return BLOG_POSTS.filter(
    (p) => !categorySlug || p.category.slug === categorySlug,
  ).map((p) => ({
    id: p.id,
    slug: p.slug,
    title: p.title,
    description: p.description,
    publishedDate: p.publishedDate,
    category: { title: p.category.title, url: p.category.url },
    hue: p.hue,
  }));
}

export function findBlogPost(slug: string): BlogPost | null {
  return BLOG_POSTS.find((p) => p.slug === slug) ?? null;
}

export function relatedBlogPosts(post: BlogPost, limit = 3) {
  return BLOG_POSTS.filter(
    (p) => p.slug !== post.slug && p.category.slug === post.category.slug,
  )
    .concat(BLOG_POSTS.filter((p) => p.slug !== post.slug))
    .filter((p, i, arr) => arr.findIndex((x) => x.slug === p.slug) === i)
    .slice(0, limit)
    .map((p) => ({
      id: p.id,
      title: p.title,
      publishedDate: p.publishedDate,
      image: { url: "" },
      category: { title: p.category.title, slug: p.category.slug, url: p.category.url },
      url: `/blog/${p.slug}`,
    }));
}
