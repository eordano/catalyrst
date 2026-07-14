import type { CSSProperties, FormEvent } from "react";
import { useState } from "react";
import SearchField from "../../atoms/SearchField";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { BLOG_CATEGORIES } from "../../data/blogCategories";
import "./stblogpost.css";

const CATEGORIES = BLOG_CATEGORIES;

type PostBlock = {
  type?: string;
  content?: string;
  items?: string[];
};

type PostDetail = {
  id?: string;
  slug?: string;
  title: string;
  description?: string;
  publishedDate?: string;
  image?: { url?: string; width?: number; height?: number };
  category: { id?: string; title: string; slug?: string; url: string };
  author?: {
    id?: string;
    title?: string;
    slug?: string;
    image?: { url?: string };
    url?: string;
  };
  body: PostBlock[];
};

type RelatedPost = {
  id: string;
  title: string;
  publishedDate: string;
  image: { url: string };
  category: { title: string; slug?: string; url: string };
  url: string;
};

const CARD_GRADS = [
  "linear-gradient(160deg, #ff2d55 0%, #ffa25a 100%)",
  "linear-gradient(160deg, #c640cd 0%, #691fa9 100%)",
  "linear-gradient(160deg, #438fff 0%, #34ce76 100%)",
];

const XGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M17.5 3h2.7l-5.9 6.7L21.3 21h-5.4l-4.3-5.6L6.7 21H4l6.3-7.2L3 3h5.5l3.9 5.1L17.5 3Zm-1 16.2h1.5L7.6 4.7H6l10.5 14.5Z" fill="currentColor" />
  </svg>
);

const FacebookGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M22 12a10 10 0 1 0-11.6 9.9v-7H7.9V12h2.5V9.8c0-2.5 1.5-3.9 3.8-3.9 1.1 0 2.2.2 2.2.2v2.5h-1.2c-1.2 0-1.6.8-1.6 1.6V12h2.7l-.4 2.9h-2.3v7A10 10 0 0 0 22 12Z" fill="currentColor" />
  </svg>
);

function PostBody({ body }: { body: PostBlock[] }) {
  return (
    <div className="stblogpost__body">
      {body.map((block, i) => {
        switch (block.type) {
          case "h2":
            return <h2 key={i}>{block.content}</h2>;
          case "h3":
            return <h3 key={i}>{block.content}</h3>;
          case "h4":
            return <h4 key={i}>{block.content}</h4>;
          case "quote":
            return <blockquote key={i}>{block.content}</blockquote>;
          case "ul":
            return (
              <ul key={i}>
                {(block.items ?? []).map((it, j) => (
                  <li key={j}>{it}</li>
                ))}
              </ul>
            );
          case "p":
          default:
            return <p key={i}>{block.content}</p>;
        }
      })}
    </div>
  );
}

function RelatedCard({ post, grad }: { post: RelatedPost; grad?: string }) {
  return (
    <article className="stblogpost__card">
      <a className="stblogpost__cardimglink" href={post.url}>
        <span
          className="stblogpost__cardimg"
          style={post.image.url ? { backgroundImage: `url(${post.image.url})` } : { backgroundImage: grad }}
          role="img"
          aria-label={post.title}
        />
      </a>
      <div>
        <p className="stblogpost__cardmeta">
          <span className="stblogpost__carddate">{post.publishedDate}</span>
          <span>
            <a className="stblogpost__cardcat" href={post.category.url}>
              {post.category.title}
            </a>
          </span>
        </p>
        <a className="stblogpost__cardtitle" href={post.url}>
          <h2>{post.title}</h2>
        </a>
      </div>
    </article>
  );
}

function BlogNavigation({ activeCategory }: { activeCategory?: string }) {
  const [query, setQuery] = useState("");

  function submitSearch(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const q = query.trim();
    window.location.assign(q ? `/blog?q=${encodeURIComponent(q)}` : "/blog");
  }

  return (
    <nav className="stblogpost__nav" aria-label="Blog categories">
      <div className="stblogpost__navcontent">
        <div className="stblogpost__navwrap">
          <div className="stblogpost__cats">
            <ul className="stblogpost__catlist">
              {CATEGORIES.map((c) => {
                const path = c.slug ? `/blog?category=${c.slug}` : "/blog";
                const isActive = c.slug === activeCategory || (!c.slug && !activeCategory);
                return (
                  <li className="stblogpost__catitem" key={c.id}>
                    <a
                      href={path}
                      className={"stblogpost__catlink" + (isActive ? " is-active" : "")}
                      aria-current={isActive ? "page" : undefined}
                    >
                      {c.title}
                    </a>
                  </li>
                );
              })}
            </ul>
          </div>
          <form className="stblogpost__search" role="search" onSubmit={submitSearch}>
            <SearchField placeholder="Search articles" value={query} onChange={setQuery} />
          </form>
        </div>
      </div>
    </nav>
  );
}

type BlogPostState = "ready" | "loading" | "error";

type StBlogPostProps = {
  chrome?: boolean;
  post?: PostDetail;
  related?: RelatedPost[];
  state?: BlogPostState;
};

export default function StBlogPost({ chrome = true, post, related = [], state = "ready" }: StBlogPostProps) {
  const activeCategory = post?.category?.slug;

  return (
    <SitesChromeMaybe chrome={chrome} active="learn">
      <div className="stblogpost">
        <div className="stblogpost__layout">
          <BlogNavigation activeCategory={activeCategory} />

          {state === "loading" && (
            <div className="stblogpost__contentwrap">
              <div className="stblogpost__centered">
                <div className="stblogpost__spinner" role="status" aria-label="Loading post" />
              </div>
            </div>
          )}

          {state === "error" && (
            <div className="stblogpost__contentwrap">
              <div className="stblogpost__centered">
                <p className="stblogpost__error">There was an error loading the post.</p>
              </div>
            </div>
          )}

          {state === "ready" && !post && (
            <div className="stblogpost__contentwrap">
              <div className="stblogpost__centered">
                <p className="stblogpost__error">No post to show.</p>
              </div>
            </div>
          )}

          {state === "ready" && post && (
            <>
              <div className="stblogpost__contentwrap">
                <article className="stblogpost__content">
                  {post.image?.url ? (
                    <img
                      className="stblogpost__image"
                      src={post.image.url}
                      alt={post.title}
                      width={post.image.width}
                      height={post.image.height}
                    />
                  ) : (
                    <div
                      className="stblogpost__image stblogpost__image--ph"
                      role="img"
                      aria-label={post.title}
                    />
                  )}

                  <header className="stblogpost__header">
                    <p className="stblogpost__meta">
                      {post.publishedDate}
                      <span className="stblogpost__metasep">&#x2022;</span>
                      <a className="stblogpost__catmeta" href={post.category.url}>
                        {post.category.title}
                      </a>
                    </p>
                    <div className="stblogpost__titlebox">
                      <h1 className="stblogpost__title">{post.title}</h1>
                    </div>
                    <p className="stblogpost__subtitle">{post.description}</p>
                  </header>

                  {post.author && post.author.title && (
                    <div className="stblogpost__authorrow">
                      <div>
                        <a className="stblogpost__authorlink" href={post.author.url}>
                          <span
                            className="stblogpost__avatar"
                            style={post.author.image?.url ? ({ backgroundImage: `url(${post.author.image.url})` } as CSSProperties) : undefined}
                            aria-hidden="true"
                          />
                          <span className="stblogpost__authorname">{post.author.title}</span>
                        </a>
                      </div>
                      <div className="stblogpost__share">
                        <span className="stblogpost__sharelabel">Share</span>
                        <a className="stblogpost__sharelink" href={`https://x.com/intent/post?text=${encodeURIComponent(post.title)}&url=${encodeURIComponent(`https://decentraland.org/blog/${post.slug}`)}`} target="_blank" rel="noopener noreferrer" aria-label="Share on X">
                          <XGlyph />
                        </a>
                        <a className="stblogpost__sharelink" href={`https://www.facebook.com/sharer/sharer.php?u=${encodeURIComponent(`https://decentraland.org/blog/${post.slug}`)}`} target="_blank" rel="noopener noreferrer" aria-label="Share on Facebook">
                          <FacebookGlyph />
                        </a>
                      </div>
                    </div>
                  )}

                  <PostBody body={post.body} />
                </article>
              </div>

              {related && related.length > 0 && (
                <section className="stblogpost__related">
                  <div className="stblogpost__relatedinner">
                    <h2 className="stblogpost__relatedtitle">Related Posts</h2>
                    <div className="stblogpost__relatedwrap">
                      {related.slice(0, 3).map((rp, i) => (
                        <RelatedCard key={rp.id} post={rp} grad={CARD_GRADS[i % CARD_GRADS.length]} />
                      ))}
                    </div>
                  </div>
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
