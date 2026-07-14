---
id: blog
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A browseable blog index with deep-linkable posts increases marketing-content
    reads and downstream download intent.
  because: >-
    A scannable index of titled, dated, categorized cards plus shareable per-post
    URLs lowers the cost of finding and reading content, so a larger share of
    index loads convert into a post read.
metric:
  primary: lp_blog_post_ctr
  numerator: lp_blog_post_clicked
  denominator: lp_blog_viewed
  guardrails:
    - lp_blog_viewed
    - lp_blog_post_viewed
experiment:
  key: lp_blog_index
  unit: session
  variants:
    - id: index_grid
      weight: 1
      flags:
        mainPostHero: true
  baseline: 0.22
  mde: 0.05
decision:
  rule: >-
    Ship the blog index + post layout if lp_blog_post_ctr (lp_blog_post_clicked /
    lp_blog_viewed) clears the baseline with 95% confidence and no guardrail
    regresses; otherwise iterate on the index grid.
---

# Story -- Blog list -> post

`/blog` renders the ui3 `StBlogHome` fed by a LOCAL content map
(`app/lib/content/blog.ts` -- minimal-dep, no CMS; the real blog is Contentful and
has no catalyst crate). `/blog/:slug` renders `StBlogPost` for a matched post;
an unknown slug (e.g. a category link) shows a graceful empty state.

- `lp_blog_viewed` fires on the index load (`{ post_count }`).
- A post-card click emits `lp_blog_post_clicked` (`{ slug }`).
- The post page emits `lp_blog_post_viewed` (`{ slug }`).

Primary metric `lp_blog_post_ctr = lp_blog_post_clicked / lp_blog_viewed`.
DEFERRED: wiring a real CMS (Contentful) -- content is static for now.
