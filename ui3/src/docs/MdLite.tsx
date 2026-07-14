import { createElement, useMemo, type ReactNode } from "react";

export type MdLiteProps = {
  source: string;
  className?: string;
};

type ListItem = { text: string; children: Block[] };

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "para"; text: string }
  | { kind: "code"; lang: string; code: string }
  | { kind: "list"; ordered: boolean; items: ListItem[] }
  | { kind: "quote"; children: Block[] }
  | { kind: "rule" }
  | { kind: "table"; header: string[]; rows: string[][] };

const FENCE_RE = /^\s{0,3}```\s*(\S*)\s*$/;
const HEADING_RE = /^(#{1,6})\s+(.*)$/;
const RULE_RE = /^\s{0,3}([-*_])\s*(?:\1\s*){2,}$/;
const QUOTE_RE = /^\s{0,3}>\s?(.*)$/;
const MARKER_RE = /^(\s*)([-*+]|\d{1,3}[.)])\s+(.*)$/;
const TABLE_SEP_RE = /^\s*\|?[\s:|-]*-[\s:|-]*\|?\s*$/;

function isBlank(line: string): boolean {
  return line.trim().length === 0;
}

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  return s.split("|").map((c) => c.trim());
}

function dedent(lines: string[]): string[] {
  let min = Infinity;
  for (const l of lines) {
    if (isBlank(l)) continue;
    const indent = l.length - l.trimStart().length;
    if (indent < min) min = indent;
  }
  if (!Number.isFinite(min) || min === 0) return lines;
  return lines.map((l) => (isBlank(l) ? "" : l.slice(min)));
}

function parseListBlock(lines: string[]): Block {
  const first = (lines[0] ?? "").match(MARKER_RE);
  const baseIndent = (first?.[1] ?? "").length;
  const ordered = /^\d/.test(first?.[2] ?? "");
  const items: ListItem[] = [];
  let textLines: string[] = [];
  let childLines: string[] = [];
  let started = false;

  const flush = () => {
    if (!started) return;
    const children = childLines.some((l) => !isBlank(l))
      ? parseBlocks(dedent(childLines))
      : [];
    items.push({ text: textLines.join(" ").trim(), children });
    textLines = [];
    childLines = [];
  };

  for (const line of lines) {
    const m = line.match(MARKER_RE);
    if (m && (m[1] ?? "").length <= baseIndent) {
      flush();
      started = true;
      textLines = [m[3] ?? ""];
    } else if (isBlank(line)) {
      if (childLines.length > 0) childLines.push("");
    } else if (m) {
      childLines.push(line);
    } else if (childLines.some((l) => !isBlank(l))) {
      childLines.push(line);
    } else {
      textLines.push(line.trim());
    }
  }
  flush();
  return { kind: "list", ordered, items };
}

function parseBlocks(lines: string[]): Block[] {
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i] ?? "";

    if (isBlank(line)) {
      i += 1;
      continue;
    }

    const fence = line.match(FENCE_RE);
    if (fence) {
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !FENCE_RE.test(lines[i] ?? "")) {
        code.push(lines[i] ?? "");
        i += 1;
      }
      i += 1;
      blocks.push({ kind: "code", lang: fence[1] ?? "", code: code.join("\n") });
      continue;
    }

    const heading = line.match(HEADING_RE);
    if (heading) {
      blocks.push({
        kind: "heading",
        level: (heading[1] ?? "#").length,
        text: (heading[2] ?? "").trim(),
      });
      i += 1;
      continue;
    }

    if (RULE_RE.test(line)) {
      blocks.push({ kind: "rule" });
      i += 1;
      continue;
    }

    if (QUOTE_RE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && QUOTE_RE.test(lines[i] ?? "")) {
        inner.push((lines[i] ?? "").match(QUOTE_RE)?.[1] ?? "");
        i += 1;
      }
      blocks.push({ kind: "quote", children: parseBlocks(inner) });
      continue;
    }

    if (MARKER_RE.test(line)) {
      const collected: string[] = [];
      while (i < lines.length) {
        const l = lines[i] ?? "";
        if (isBlank(l)) {
          const next = lines[i + 1];
          if (next !== undefined && (MARKER_RE.test(next) || /^\s{2,}\S/.test(next))) {
            collected.push(l);
            i += 1;
            continue;
          }
          break;
        }
        if (MARKER_RE.test(l) || /^\s{2,}\S/.test(l)) {
          collected.push(l);
          i += 1;
          continue;
        }
        break;
      }
      blocks.push(parseListBlock(collected));
      continue;
    }

    const nextLine = lines[i + 1];
    if (
      line.includes("|") &&
      nextLine !== undefined &&
      nextLine.includes("-") &&
      TABLE_SEP_RE.test(nextLine)
    ) {
      const header = splitRow(line);
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length) {
        const l = lines[i] ?? "";
        if (!l.includes("|") || isBlank(l)) break;
        rows.push(splitRow(l));
        i += 1;
      }
      blocks.push({ kind: "table", header, rows });
      continue;
    }

    const para: string[] = [line.trim()];
    i += 1;
    while (i < lines.length) {
      const l = lines[i] ?? "";
      if (
        isBlank(l) ||
        HEADING_RE.test(l) ||
        FENCE_RE.test(l) ||
        RULE_RE.test(l) ||
        QUOTE_RE.test(l) ||
        MARKER_RE.test(l)
      ) {
        break;
      }
      para.push(l.trim());
      i += 1;
    }
    blocks.push({ kind: "para", text: para.join(" ") });
  }

  return blocks;
}

type InlineMatch = { start: number; end: number; node: (key: number) => ReactNode };

const CODE_RE = /`([^`]+)`/;
const LINK_RE = /\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/;
const BOLD_RE = /\*\*([^*]+(?:\*[^*]+)*)\*\*|__([^_]+)__/;
const ITALIC_RE = /(?<![\w*\\])\*([^*\s][^*]*?)\*(?!\*)|(?<![\w`])_([^_]+)_(?!\w)/;

function firstMatch(text: string): InlineMatch | null {
  let best: InlineMatch | null = null;

  const code = CODE_RE.exec(text);
  if (code) {
    const content = code[1] ?? "";
    best = {
      start: code.index,
      end: code.index + code[0].length,
      node: (k) => <code key={k}>{content}</code>,
    };
  }

  const link = LINK_RE.exec(text);
  if (link && (!best || link.index < best.start)) {
    const label = link[1] ?? "";
    const href = link[2] ?? "#";
    best = {
      start: link.index,
      end: link.index + link[0].length,
      node: (k) => (
        <a key={k} href={href}>
          {renderInline(label)}
        </a>
      ),
    };
  }

  const bold = BOLD_RE.exec(text);
  if (bold && (!best || bold.index < best.start)) {
    const content = bold[1] ?? bold[2] ?? "";
    best = {
      start: bold.index,
      end: bold.index + bold[0].length,
      node: (k) => <strong key={k}>{renderInline(content)}</strong>,
    };
  }

  const italic = ITALIC_RE.exec(text);
  if (italic && (!best || italic.index < best.start)) {
    const content = italic[1] ?? italic[2] ?? "";
    best = {
      start: italic.index,
      end: italic.index + italic[0].length,
      node: (k) => <em key={k}>{renderInline(content)}</em>,
    };
  }

  return best;
}

function renderInline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (rest.length > 0) {
    const m = firstMatch(rest);
    if (!m) {
      out.push(rest);
      break;
    }
    if (m.start > 0) out.push(rest.slice(0, m.start));
    out.push(m.node(key));
    key += 1;
    rest = rest.slice(m.end);
  }
  return out;
}

function renderBlocks(blocks: Block[]): ReactNode[] {
  return blocks.map((b, i) => {
    switch (b.kind) {
      case "heading":
        return createElement(
          `h${Math.min(6, Math.max(1, b.level))}`,
          { key: i },
          renderInline(b.text),
        );
      case "para":
        return <p key={i}>{renderInline(b.text)}</p>;
      case "code":
        return (
          <pre key={i} data-lang={b.lang || undefined}>
            <code>{b.code}</code>
          </pre>
        );
      case "list": {
        const items = b.items.map((it, j) => (
          <li key={j}>
            {renderInline(it.text)}
            {it.children.length > 0 ? renderBlocks(it.children) : null}
          </li>
        ));
        return b.ordered ? <ol key={i}>{items}</ol> : <ul key={i}>{items}</ul>;
      }
      case "quote":
        return <blockquote key={i}>{renderBlocks(b.children)}</blockquote>;
      case "rule":
        return <hr key={i} />;
      case "table":
        // The wrapper, not the table, scrolls: a scrollable region needs
        // keyboard access (axe scrollable-region-focusable), and display:block
        // on the table itself would strip its table semantics.
        return (
          <div key={i} className="mdlite__tablescroll" tabIndex={0}>
            <table>
              <thead>
                <tr>
                  {b.header.map((h, j) => (
                    <th key={j}>{renderInline(h)}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {b.rows.map((row, j) => (
                  <tr key={j}>
                    {row.map((cell, c) => (
                      <td key={c}>{renderInline(cell)}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
    }
  });
}

export default function MdLite({ source, className }: MdLiteProps) {
  const blocks = useMemo(
    () => parseBlocks(source.replace(/\r\n/g, "\n").split("\n")),
    [source],
  );
  return (
    <div className={"mdlite" + (className ? " " + className : "")}>
      {renderBlocks(blocks)}
    </div>
  );
}
