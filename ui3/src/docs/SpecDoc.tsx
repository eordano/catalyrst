import MdLite from "./MdLite";
import "./specdoc.css";

export type SpecDocProps = {
  source: string;
  path?: string;
};

function splitFrontMatter(source: string): { meta: string | null; body: string } {
  const norm = source.replace(/\r\n/g, "\n");
  if (!norm.startsWith("---\n")) return { meta: null, body: norm };
  const end = norm.indexOf("\n---\n", 4);
  if (end === -1) return { meta: null, body: norm };
  return { meta: norm.slice(4, end), body: norm.slice(end + 5) };
}

export default function SpecDoc({ source, path }: SpecDocProps) {
  const { meta, body } = splitFrontMatter(source);
  return (
    <main className="specdoc">
      <article className="specdoc__page">
        {path ? <p className="specdoc__path">{path}</p> : null}
        {meta ? (
          <details className="specdoc__meta">
            <summary>Experiment front-matter</summary>
            <pre>
              <code>{meta}</code>
            </pre>
          </details>
        ) : null}
        <MdLite source={body} />
      </article>
    </main>
  );
}
