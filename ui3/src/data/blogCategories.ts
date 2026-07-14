export type BlogCategory = { id: string; slug: string; title: string };

export type BlogCategoryView = BlogCategory & { root: boolean; active: boolean };

export const BLOG_CATEGORIES: BlogCategory[] = [
  { id: "all", slug: "", title: "All articles" },
  { id: "c-ann", slug: "announcements", title: "Announcements" },
  { id: "c-plat", slug: "platform", title: "Platform" },
  { id: "c-comm", slug: "community", title: "Community" },
  { id: "c-eco", slug: "ecosystem", title: "Ecosystem" },
  { id: "c-tut", slug: "tutorials", title: "Tutorials" },
];

export const blogCategories = (activeSlug = ""): BlogCategoryView[] =>
  BLOG_CATEGORIES.map((c) => ({ ...c, root: c.slug === "", active: c.slug === activeSlug }));
