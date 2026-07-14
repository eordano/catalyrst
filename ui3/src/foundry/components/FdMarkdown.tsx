import type { ReactNode } from "react";

import FdScrollTable from "./FdScrollTable";
import "./fdmarkdown.css";

// Code span, site-relative or https link, bold, italic. A run that matches none
// of them stays exactly as it was written — including the honesty markers,
// which are body text.
const INLINE =
  /`[^`]+`|\[[^\]]+\]\((?:\/[^)\s]+|#[^)\s]+|https:\/\/[^)\s]+)\)|\*\*[^*]+\*\*|\*[^\s*][^*\n]*?\*/g;
const LINK = /^\[([^\]]+)\]\((.+)\)$/;

// Stored bodies stay verbatim (exports and the edit textarea read the raw
// markdown); only DISPLAY normalizes the one TeX habit the imports carry.
const TEX_ARROWS: readonly [RegExp, string][] = [
  [/\$\\(?:longrightarrow|rightarrow|to)\$/g, "→"],
  [/\$\\(?:Rightarrow|implies)\$/g, "⇒"],
  [/\$\\(?:leftarrow|gets)\$/g, "←"],
  [/\$\\leftrightarrow\$/g, "↔"],
];

function inlineMd(raw: string): ReactNode {
  const text = TEX_ARROWS.reduce((t, [re, ch]) => t.replace(re, ch), raw);
  const nodes: ReactNode[] = [];
  const re = new RegExp(INLINE.source, "g");
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) nodes.push(text.slice(last, m.index));
    const tok = m[0];
    const key = nodes.length;
    if (tok.startsWith("`")) {
      nodes.push(
        <code key={key} className="fd-gdd__inlinecode">
          {tok.slice(1, -1)}
        </code>,
      );
    } else if (tok.startsWith("[")) {
      const link = LINK.exec(tok);
      const label = link?.[1];
      const href = link?.[2];
      nodes.push(
        label !== undefined && href !== undefined ? (
          <a key={key} href={href}>
            {label}
          </a>
        ) : (
          tok
        ),
      );
    } else if (tok.startsWith("**")) {
      nodes.push(<strong key={key}>{inlineMd(tok.slice(2, -2))}</strong>);
    } else {
      nodes.push(<em key={key}>{inlineMd(tok.slice(1, -1))}</em>);
    }
    last = m.index + tok.length;
  }
  if (nodes.length === 0) return text;
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

function rowCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());
}

const DELIMITER = /^:?-{2,}:?$/;

function mdBlocks(md: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = md.split("\n");
  let para: string[] = [];
  let quote: string[] = [];
  let bullets: string[] = [];
  let numbered: { start: number; items: string[] } | null = null;
  let table: string[] | null = null;
  let fence: string[] | null = null;

  const flushPara = () => {
    if (para.length === 0) return;
    out.push(<p key={out.length}>{inlineMd(para.join("\n"))}</p>);
    para = [];
  };
  const flushQuote = () => {
    if (quote.length === 0) return;
    const paras = quote
      .join("\n")
      .split(/\n\s*\n/)
      .filter((p) => p.trim() !== "");
    out.push(
      <blockquote key={out.length} className="fd-gdd__quote">
        {paras.map((p, i) => (
          <p key={i}>{inlineMd(p)}</p>
        ))}
      </blockquote>,
    );
    quote = [];
  };
  const flushBullets = () => {
    if (bullets.length === 0) return;
    out.push(
      <ul key={out.length}>
        {bullets.map((item, i) => (
          <li key={i}>{inlineMd(item)}</li>
        ))}
      </ul>,
    );
    bullets = [];
  };
  const flushNumbered = () => {
    if (!numbered) return;
    const { start, items } = numbered;
    out.push(
      <ol key={out.length} start={start}>
        {items.map((item, i) => (
          <li key={i}>{inlineMd(item)}</li>
        ))}
      </ol>,
    );
    numbered = null;
  };
  const flushTable = () => {
    if (!table) return;
    const rows = table.map(rowCells);
    const head = rows[0];
    const delimiter = rows[1];
    if (
      !head ||
      !delimiter ||
      delimiter.length === 0 ||
      !delimiter.every((c) => DELIMITER.test(c))
    ) {
      // Pipes that never declared a table: the lines stay as they were typed.
      for (const line of table) out.push(<p key={out.length}>{inlineMd(line)}</p>);
      table = null;
      return;
    }
    const body = rows.slice(2);
    // A blank header row is the shortGDD's key/value table: no column names,
    // the first cell of each row names it.
    const keyed = head.every((c) => c === "");
    out.push(
      <FdScrollTable key={out.length} ariaLabel="Table from the document">
        <table className="fd-table">
          {keyed ? null : (
            <thead>
              <tr>
                {head.map((c, i) => (
                  <th key={i} scope="col">
                    {inlineMd(c)}
                  </th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {body.map((cells, r) => (
              <tr key={r}>
                {cells.map((c, i) =>
                  keyed && i === 0 ? (
                    <th key={i} scope="row">
                      {inlineMd(c)}
                    </th>
                  ) : (
                    <td key={i} className="fd-table__prose">
                      {inlineMd(c)}
                    </td>
                  ),
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </FdScrollTable>,
    );
    table = null;
  };
  const flushAll = () => {
    flushTable();
    flushQuote();
    flushBullets();
    flushNumbered();
    flushPara();
  };

  for (const line of lines) {
    if (fence !== null) {
      if (/^\s*```/.test(line)) {
        out.push(
          <pre key={out.length} className="fd-code">
            {fence.join("\n")}
          </pre>,
        );
        fence = null;
      } else {
        fence.push(line);
      }
      continue;
    }
    if (/^\s*```/.test(line)) {
      flushAll();
      fence = [];
      continue;
    }

    if (/^\s*\|/.test(line)) {
      if (!table) {
        flushQuote();
        flushBullets();
        flushNumbered();
        flushPara();
        table = [];
      }
      table.push(line);
      continue;
    }
    flushTable();

    const heading = /^(#{1,6})\s+(.*\S)\s*$/.exec(line);
    if (heading) {
      flushAll();
      const text = heading[2] ?? "";
      out.push(
        (heading[1] ?? "").length <= 2 ? (
          <h4 key={out.length} className="fd-subhead fd-gdd__dochead">
            {text}
          </h4>
        ) : (
          <h5 key={out.length} className="fd-gdd__docsubhead">
            {text}
          </h5>
        ),
      );
      continue;
    }

    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      flushAll();
      out.push(<hr key={out.length} className="fd-gdd__rule" />);
      continue;
    }

    const quoted = /^\s*>\s?(.*)$/.exec(line);
    if (quoted) {
      flushBullets();
      flushNumbered();
      flushPara();
      quote.push(quoted[1] ?? "");
      continue;
    }
    flushQuote();

    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      flushNumbered();
      flushPara();
      bullets.push(bullet[1] ?? "");
      continue;
    }
    const numberedItem = /^\s*(\d+)\.\s+(.*)$/.exec(line);
    if (numberedItem) {
      flushBullets();
      flushPara();
      if (!numbered) numbered = { start: Number(numberedItem[1]), items: [] };
      numbered.items.push(numberedItem[2] ?? "");
      continue;
    }
    flushBullets();
    flushNumbered();

    if (line.trim() === "") {
      flushPara();
      continue;
    }
    para.push(line);
  }

  if (fence !== null) {
    out.push(
      <pre key={out.length} className="fd-code">
        {fence.join("\n")}
      </pre>,
    );
  }
  flushAll();
  return out;
}

/** Markdown as the document's own shapes: headings, tables, quotes, lists,
 *  rules, code. A block this does not recognise renders as its own lines, so
 *  nothing written is dropped. */
export default function FdMarkdown({ source }: { source: string }) {
  return <>{mdBlocks(source)}</>;
}
