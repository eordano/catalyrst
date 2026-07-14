import type { CSSProperties } from "react";
import { useEffect, useMemo, useState } from "react";
import { blogCategories } from "../../data/blogCategories";
import type { BlogCategoryView } from "../../data/blogCategories";
import SearchField from "../../atoms/SearchField";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./stbloghome.css";

const poster = (hue: number): CSSProperties => ({
  backgroundImage: `linear-gradient(150deg, hsl(${hue} 70% 52%) 0%, hsl(${(hue + 40) % 360} 60% 28%) 100%)`,
});

const CATEGORIES = blogCategories("");

export type BlogPost = {
  id: string;
  title: string;
  description: string;
  publishedDate: string;
  category: { title: string; url: string };
  hue: number;
};

function BlogNavigation({
  categories,
  onSearch,
  query,
  labelSuffix,
}: LabelSuffixProps & {
  categories: BlogCategoryView[];
  onSearch: (value: string) => void;
  query: string;
}) {
  return (
    <div className="stbloghome__nav">
      <div className="stbloghome__navcontent">
        <div className="stbloghome__navwrap">
          <nav className="stbloghome__cats" aria-label={suffixLabel("Blog categories", labelSuffix)}>
            <ul className="stbloghome__catlist">
              {categories.map((c) => (
                <li className="stbloghome__catitem" key={c.id}>
                  <a
                    className={"stbloghome__catlink" + (c.active ? " is-active" : "")}
                    href={c.slug ? `/blog?category=${c.slug}` : "/blog"}
                    aria-current={c.active ? "page" : undefined}
                  >
                    {c.title}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
          <div className="stbloghome__search">
            <SearchField placeholder="Search..." value={query} onChange={onSearch} />
          </div>
        </div>
      </div>
    </div>
  );
}

function PostCard({ post }: { post: BlogPost }) {
  return (
    <article className="stbloghome__card">
      <a className="stbloghome__cardimglink" href={`/blog/${post.id}`}>
        <span className="stbloghome__cardimg" style={poster(post.hue)} role="img" aria-label={post.title} />
      </a>
      <div className="stbloghome__cardinfo">
        <div className="stbloghome__meta">
          <span className="stbloghome__date">{post.publishedDate}</span>
          <span>
            <a className="stbloghome__catlnk" href={post.category.url}>
              {post.category.title}
            </a>
          </span>
        </div>
        <a className="stbloghome__titlelink" href={`/blog/${post.id}`}>
          <h2 className="stbloghome__cardtitle">{post.title}</h2>
        </a>
      </div>
    </article>
  );
}

function MainPostCard({ post }: { post: BlogPost }) {
  return (
    <div className="stbloghome__main">
      <a className="stbloghome__mainimglink" href={`/blog/${post.id}`}>
        <span className="stbloghome__mainimg" style={poster(post.hue)} role="img" aria-label={post.title} />
      </a>
      <div className="stbloghome__maininfo">
        <div className="stbloghome__meta">
          <span className="stbloghome__date">{post.publishedDate}</span>
          <span>
            <a className="stbloghome__catlnk" href={post.category.url}>
              {post.category.title}
            </a>
          </span>
        </div>
        <a className="stbloghome__titlelink" href={`/blog/${post.id}`}>
          <h2 className="stbloghome__maintitle">{post.title}</h2>
        </a>
        <p className="stbloghome__maindesc">{post.description}</p>
      </div>
    </div>
  );
}

function MainPostSkeleton() {
  return (
    <div className="stbloghome__main">
      <span className="stbloghome__skel stbloghome__skel--mainimg" />
      <div className="stbloghome__maininfo">
        <div className="stbloghome__skelhead">
          <span className="stbloghome__skel stbloghome__skel--meta" />
          <span className="stbloghome__skel stbloghome__skel--meta" />
        </div>
        <span className="stbloghome__skel stbloghome__skel--titleline" />
        <span className="stbloghome__skel stbloghome__skel--titleline stbloghome__skel--short" />
        <div className="stbloghome__skelbody">
          <span className="stbloghome__skel stbloghome__skel--text" />
          <span className="stbloghome__skel stbloghome__skel--text" />
          <span className="stbloghome__skel stbloghome__skel--text" />
          <span className="stbloghome__skel stbloghome__skel--text stbloghome__skel--short" />
        </div>
      </div>
    </div>
  );
}

function PostCardSkeleton() {
  return (
    <article className="stbloghome__card">
      <span className="stbloghome__skel stbloghome__skel--cardimg" />
      <div className="stbloghome__cardinfo">
        <div className="stbloghome__skelhead">
          <span className="stbloghome__skel stbloghome__skel--meta" />
          <span className="stbloghome__skel stbloghome__skel--meta" />
        </div>
        <span className="stbloghome__skel stbloghome__skel--text" />
        <span className="stbloghome__skel stbloghome__skel--text stbloghome__skel--short" />
      </div>
    </article>
  );
}

function PostList({
  posts,
  loading,
  hasMainPost,
  query,
  onClearSearch,
}: {
  posts: BlogPost[];
  loading: boolean;
  hasMainPost: boolean;
  query: string;
  onClearSearch: () => void;
}) {
  if (loading && posts.length === 0) {
    const count = hasMainPost ? 7 : 6;
    return (
      <div className={"stbloghome__list" + (hasMainPost ? " has-main" : "")}>
        {hasMainPost && <MainPostSkeleton />}
        {Array.from({ length: count }, (_, i) =>
          hasMainPost && i === 0 ? null : <PostCardSkeleton key={`sk-${i}`} />
        )}
      </div>
    );
  }

  if (!posts || posts.length === 0) {
    const searching = query.trim().length > 0;
    return (
      <div className="stbloghome__empty" role="status">
        <h2 className="stbloghome__emptytitle">
          {searching ? "No posts match your search" : "No posts yet"}
        </h2>
        <p className="stbloghome__emptytext">
          {searching
            ? "Try a different keyword, or clear the search to see everything."
            : "Check back soon for the latest news and updates."}
        </p>
        {searching && (
          <button type="button" className="stbloghome__emptybtn" onClick={onClearSearch}>
            Clear search
          </button>
        )}
      </div>
    );
  }

  const firstRealPost = posts[0];

  return (
    <div className={"stbloghome__list" + (hasMainPost ? " has-main" : "")}>
      {hasMainPost && firstRealPost && <MainPostCard post={firstRealPost} />}
      {posts.map((post) => {
        if (hasMainPost && post === firstRealPost) return null;
        return <PostCard key={post.id} post={post} />;
      })}
    </div>
  );
}

type StBlogHomeProps = LabelSuffixProps & {
  chrome?: boolean;
  posts?: BlogPost[];
  categories?: BlogCategoryView[];
  loading?: boolean;
  error?: boolean;
  hasMainPost?: boolean;
};

export default function StBlogHome({
  labelSuffix,
  chrome = true,
  posts = [],
  categories = CATEGORIES,
  loading = false,
  error = false,
  hasMainPost = true,
}: StBlogHomeProps) {
  const [query, setQuery] = useState("");
  const isLoadingInitial = useMemo(() => loading && posts.length === 0, [loading, posts]);

  useEffect(() => {
    try {
      const q = new URLSearchParams(window.location.search).get("q") ?? "";
      if (q) setQuery(q);
    } catch {
    }
  }, []);

  const visiblePosts = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return posts;
    return posts.filter(
      (p) =>
        p.title.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.category.title.toLowerCase().includes(q)
    );
  }, [posts, query]);

  return (
    <SitesChromeMaybe chrome={chrome} active="learn">
      <div className="stbloghome">
        <BlogNavigation categories={categories} onSearch={setQuery} query={query} labelSuffix={labelSuffix} />
        <div className="stbloghome__content">
          {error ? (
            <div className="stbloghome__error">
              <span className="stbloghome__errortext">Failed to load posts. Please try again later.</span>
            </div>
          ) : (
            <PostList
              posts={visiblePosts}
              loading={isLoadingInitial}
              hasMainPost={hasMainPost}
              query={query}
              onClearSearch={() => setQuery("")}
            />
          )}
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
