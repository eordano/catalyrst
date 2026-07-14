/**
 * Bans zod defaults that make `safeParse` incapable of failing.
 *
 * The bug this exists to prevent, found in `wcs.ts` and shipped briefly:
 *
 *     const LiveDataSchema = z.object({
 *       data: z.object({ totalUsers: z.number(), perWorld: z.array(...) })
 *         .nullish()
 *         .transform((v) => v ?? { totalUsers: 0, perWorld: [] }),   // <- here
 *     });
 *
 * With that transform the schema cannot reject anything: an error body, an HTML
 * error page, a truncated response -- all parse "successfully" into a
 * valid-looking zero. The caller then has no way to tell "the upstream is
 * broken" from "the measurement is zero", and the UI renders a confident number
 * that was never measured.
 *
 * The distinction this enforces:
 *
 *     .transform((v) => v ?? null)   OK   -- null means "absent", which is true
 *     .transform((v) => v ?? 0)      BAN  -- 0 is a measurement nobody took
 *     .transform((v) => v ?? [])     BAN  -- [] means "we looked and found none"
 *     .transform((v) => v ?? {...})  BAN  -- a whole fabricated object
 *
 * An empty array is the sharp one: it is indistinguishable from a real empty
 * result, so "your request failed" renders as "you have no worlds".
 *
 * Generation does not solve this. A schema generated from a Rust type and
 * written this way would be exactly as wrong -- the defect is the default, not
 * the provenance.
 *
 * The same lie is also told in control flow, where no declarative chain shows
 * it. Two shapes, both of which let a parse fail and ship the value anyway:
 *
 *     const r = PlaceSchema.safeParse(raw);
 *     if (r.success) return r.data;
 *     reportSchemaDrift("Place", r.error.issues);
 *     return raw as Place;              // <- the parse rejected it; ship it
 *
 * A cast is not a check. Once the schema has rejected the payload, `as Place`
 * only stops the compiler from saying so: every field the schema existed to
 * guarantee is now whatever the upstream happened to send, the warning goes to
 * a console nobody reads, and the caller is handed a `Place`. This is worse
 * than a fabricated default, because the type now claims the value was
 * validated.
 *
 *     try { return await loadWearables(); } catch { return []; }
 *
 * A catch that returns a literal collapses every failure -- DNS, 500, abort, a
 * bug in the parser itself -- into a successful empty answer. It is `?? []` one
 * level up the call stack, with the same sharp edge: "we could not reach the
 * server" renders as "you own nothing". `null`, or letting the error propagate,
 * keeps the two apart, and the call site is the last place that difference can
 * still be told.
 *
 *   node scripts/check-schema-honesty.mts           # report
 *   node scripts/check-schema-honesty.mts --check   # exit 1 on any violation
 */
import { readdirSync, readFileSync, existsSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import type * as TSNamespace from "typescript6";

const require = createRequire(import.meta.url);
const ts: typeof TSNamespace = require("typescript6");

const SITES = fileURLToPath(new URL("..", import.meta.url));
// ui3 carries a SECOND schema tree (src/data/catalyst/*.ts) that parses the same
// catalyst payloads. Scanning only sites left it unguarded regardless of syntax.
const DATA_ROOT = join(SITES, "packages", "data");
const ROOTS = [
  join(DATA_ROOT, "src", "lib"),
  join(SITES, "..", "ui3", "src", "data"),
];
const ALLOWLIST_PATH = join(SITES, "scripts", "schema-honesty-allow.json");
const BASELINE_PATH = join(SITES, "scripts", "schema-honesty-baseline.json");

/**
 * Two files, deliberately not one.
 *
 * `schema-honesty-allow.json` is an EXEMPTION list: each entry claims the
 * fabricated value is actually true, and must say why. It should stay short.
 *
 * `schema-honesty-baseline.json` is a DEBT list: schemas that predate this
 * check and have not been triaged. Entries are not justified and are not
 * endorsed -- they exist so the gate can block NEW violations without demanding
 * 169 fixes first. Burn it down; never add to it by hand.
 */
type Allow = { file: string; line: number; why: string };

type Rule = "fabricated-default" | "cast-after-failed-parse" | "fabricating-catch";

interface Violation {
  file: string;
  line: number;
  text: string;
  rule: Rule;
}

function tsFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  )) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "__snapshots__") continue;
      tsFiles(full, out);
    } else if (entry.name.endsWith(".ts") && !entry.name.endsWith(".d.ts")) {
      out.push(full);
    }
  }
  return out;
}

/**
 * True when the expression fabricates a value: anything except `null` and
 * `undefined`. `?? undefined` is pointless but not a lie, so it is allowed.
 */
function fabricatesAValue(node: TSNamespace.Expression): boolean {
  if (node.kind === ts.SyntaxKind.NullKeyword) return false;
  if (ts.isIdentifier(node) && node.text === "undefined") return false;
  return true;
}

/** Wrappers that change the type of an expression but never its value. */
function unwrap(node: TSNamespace.Expression): TSNamespace.Expression {
  if (ts.isParenthesizedExpression(node)) return unwrap(node.expression);
  if (ts.isAsExpression(node)) return unwrap(node.expression);
  if (ts.isSatisfiesExpression(node)) return unwrap(node.expression);
  if (ts.isNonNullExpression(node)) return unwrap(node.expression);
  return node;
}

/**
 * The `as X` a return hands to the caller, or null when there is none.
 *
 * `as const` is skipped through: it narrows a literal the author wrote, so it
 * asserts nothing about a payload. `x as unknown as X` lands on the outer cast,
 * which is the one making the false claim.
 */
function outerCast(expr: TSNamespace.Expression): TSNamespace.AsExpression | null {
  if (ts.isParenthesizedExpression(expr)) return outerCast(expr.expression);
  if (ts.isAsExpression(expr)) {
    const t = expr.type;
    const isConst =
      ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName) && t.typeName.text === "const";
    return isConst ? outerCast(expr.expression) : expr;
  }
  return null;
}

/**
 * An object that names its own failure is the fix, not the defect.
 *
 * `catch { return { ok: false, status: 502, error } }` hands the caller the one
 * thing a fabricated `[]` withholds: that this did not work. Flagging it would
 * teach a reader to ignore the gate, so the shape it is trying to encourage is
 * exempt by construction rather than by allowlist entry.
 */
function carriesFailure(obj: TSNamespace.ObjectLiteralExpression): boolean {
  const FAILURE_WORDS = /^(error|errors|err|reason|message|issues|unavailable|unreachable|failed)$/i;
  const FAILURE_VALUES = /^(error|errored|unavailable|failed|failure|fixture)$/i;
  return obj.properties.some((p) => {
    const name = p.name && ts.isIdentifier(p.name) ? p.name.text : "";
    if (FAILURE_WORDS.test(name)) return true;
    if (!ts.isPropertyAssignment(p)) return false;
    const v = unwrap(p.initializer);
    if (/^(ok|success)$/i.test(name) && v.kind === ts.SyntaxKind.FalseKeyword) return true;
    return (
      (ts.isStringLiteral(v) || ts.isNoSubstitutionTemplateLiteral(v)) &&
      FAILURE_VALUES.test(v.text)
    );
  });
}

function fabricatesALiteral(expr: TSNamespace.Expression): boolean {
  const e = unwrap(expr);
  if (ts.isObjectLiteralExpression(e)) return !carriesFailure(e);
  if (ts.isArrayLiteralExpression(e)) return true;
  if (ts.isNumericLiteral(e)) return true;
  if (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e)) return e.text.length > 0;
  return false;
}

function isFunctionish(node: TSNamespace.Node): node is TSNamespace.FunctionLikeDeclaration {
  return (
    ts.isArrowFunction(node) ||
    ts.isFunctionExpression(node) ||
    ts.isFunctionDeclaration(node) ||
    ts.isMethodDeclaration(node) ||
    ts.isConstructorDeclaration(node) ||
    ts.isGetAccessorDeclaration(node) ||
    ts.isSetAccessorDeclaration(node)
  );
}

/** Returns that belong to this body, not to a callback declared inside it. */
function ownReturns(node: TSNamespace.Node): TSNamespace.ReturnStatement[] {
  const out: TSNamespace.ReturnStatement[] = [];
  const walk = (n: TSNamespace.Node): void => {
    if (n !== node && isFunctionish(n)) return;
    if (ts.isReturnStatement(n)) out.push(n);
    ts.forEachChild(n, walk);
  };
  walk(node);
  return out;
}

function callsSafeParse(node: TSNamespace.Node): boolean {
  let found = false;
  const walk = (n: TSNamespace.Node): void => {
    if (found) return;
    if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression)) {
      const name = n.expression.name.text;
      if (name === "safeParse" || name === "safeParseAsync") {
        found = true;
        return;
      }
    }
    ts.forEachChild(n, walk);
  };
  walk(node);
  return found;
}

/**
 * `true` when the condition holds exactly on a successful parse, `false` when
 * it holds exactly on a failed one, null when it is not about a parse result.
 */
function successPolarity(cond: TSNamespace.Expression): boolean | null {
  if (ts.isParenthesizedExpression(cond)) return successPolarity(cond.expression);
  if (ts.isPrefixUnaryExpression(cond) && cond.operator === ts.SyntaxKind.ExclamationToken) {
    const inner = successPolarity(cond.operand);
    return inner === null ? null : !inner;
  }
  if (ts.isBinaryExpression(cond)) {
    const op = cond.operatorToken.kind;
    if (
      op !== ts.SyntaxKind.EqualsEqualsEqualsToken &&
      op !== ts.SyntaxKind.EqualsEqualsToken &&
      op !== ts.SyntaxKind.ExclamationEqualsEqualsToken &&
      op !== ts.SyntaxKind.ExclamationEqualsToken
    ) {
      return null;
    }
    const negated =
      op === ts.SyntaxKind.ExclamationEqualsEqualsToken ||
      op === ts.SyntaxKind.ExclamationEqualsToken;
    const boolOf = (e: TSNamespace.Expression): boolean | null =>
      e.kind === ts.SyntaxKind.TrueKeyword
        ? true
        : e.kind === ts.SyntaxKind.FalseKeyword
          ? false
          : null;
    const left = boolOf(cond.left);
    const right = boolOf(cond.right);
    const literal = left ?? right;
    if (literal === null) return null;
    const polarity = successPolarity(left === null ? cond.left : cond.right);
    if (polarity === null) return null;
    return literal === !negated ? polarity : !polarity;
  }
  if (ts.isPropertyAccessExpression(cond) && cond.name.text === "success") return true;
  return null;
}

function alwaysExits(stmt: TSNamespace.Statement): boolean {
  if (ts.isReturnStatement(stmt) || ts.isThrowStatement(stmt)) return true;
  if (ts.isBlock(stmt)) {
    const last = stmt.statements[stmt.statements.length - 1];
    return last !== undefined && alwaysExits(last);
  }
  return false;
}

/**
 * The statements a function reaches only once a parse has failed: the body of
 * `if (!r.success)`, the `else` of `if (r.success)`, and everything after an
 * `if (r.success) return ...` guard -- the spelling nearly every parse helper in
 * this tree uses.
 */
function failureRegions(body: TSNamespace.Node): TSNamespace.Node[] {
  const out: TSNamespace.Node[] = [];

  const walkAny = (n: TSNamespace.Node): void => {
    if (isFunctionish(n)) return;
    if (ts.isBlock(n)) {
      walkBlock(n.statements);
      return;
    }
    ts.forEachChild(n, walkAny);
  };

  const walkBlock = (stmts: readonly TSNamespace.Statement[]): void => {
    for (let i = 0; i < stmts.length; i++) {
      const st = stmts[i];
      if (!ts.isIfStatement(st)) {
        walkAny(st);
        continue;
      }
      const polarity = successPolarity(st.expression);
      if (polarity === false) {
        out.push(st.thenStatement);
        if (st.elseStatement) walkAny(st.elseStatement);
        continue;
      }
      if (polarity === true) {
        if (st.elseStatement) {
          out.push(st.elseStatement);
          walkAny(st.thenStatement);
          continue;
        }
        if (alwaysExits(st.thenStatement)) {
          for (const rest of stmts.slice(i + 1)) out.push(rest);
          return;
        }
      }
      walkAny(st.thenStatement);
      if (st.elseStatement) walkAny(st.elseStatement);
    }
  };

  if (ts.isBlock(body)) walkBlock(body.statements);
  else walkAny(body);
  return out;
}

/** The expressions a function-ish argument can hand back. */
function returnedExpressions(fn: TSNamespace.Expression): TSNamespace.Expression[] {
  if (!ts.isArrowFunction(fn) && !ts.isFunctionExpression(fn)) return [];
  if (ts.isArrowFunction(fn) && !ts.isBlock(fn.body)) return [fn.body];
  const out: TSNamespace.Expression[] = [];
  fn.body?.forEachChild((c) => {
    if (ts.isReturnStatement(c) && c.expression) out.push(c.expression);
  });
  return out;
}

/**
 * True when an expression hands back a fabricated value in place of the input.
 *
 * `?? X` was the only form the original check knew, so the same lie written any
 * other way passed silently -- `||` (which also fires on "" / 0 / false), and the
 * ternary a helper factory reaches for: `num = (d) => z.number().nullish()
 * .transform((v) => (v == null ? d : v))`. One such helper hid nine call sites.
 */
function fallbackFabricates(expr: TSNamespace.Expression): boolean {
  if (ts.isParenthesizedExpression(expr)) return fallbackFabricates(expr.expression);
  if (ts.isBinaryExpression(expr)) {
    const op = expr.operatorToken.kind;
    if (op === ts.SyntaxKind.QuestionQuestionToken || op === ts.SyntaxKind.BarBarToken) {
      return fabricatesAValue(expr.right);
    }
    return false;
  }
  if (ts.isConditionalExpression(expr)) {
    // `v == null ? X : v` and its inverse `v != null ? v : X`: one arm is the
    // input passed through, the other invents a value.
    const t = expr.whenTrue;
    const f = expr.whenFalse;
    if (ts.isIdentifier(f) && fabricatesAValue(t)) return true;
    if (ts.isIdentifier(t) && fabricatesAValue(f)) return true;
  }
  return false;
}

function scan(file: string): Violation[] {
  const text = readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const found: Violation[] = [];
  const seen = new Set<number>();
  // ui3 owns its own fetch idioms and swallows errors on purpose in places the
  // server tree cannot; the catch rule is only claimed over packages/data.
  const inDataTree = !relative(DATA_ROOT, file).startsWith("..");

  const record = (node: TSNamespace.Node, rule: Rule): void => {
    const start = node.getStart(source);
    if (seen.has(start)) return;
    seen.add(start);
    const { line } = source.getLineAndCharacterOfPosition(start);
    found.push({
      file: relative(SITES, file),
      line: line + 1,
      text: node.getText(source).replace(/\s+/g, " ").slice(0, 100),
      rule,
    });
  };

  const visit = (node: TSNamespace.Node): void => {
    // Match on the method name alone. Narrowing to zod specifically would need
    // type resolution, and a fallback that invents a value is suspect anywhere.
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.arguments.length > 0
    ) {
      const method = node.expression.name.text;
      const arg = node.arguments[0];

      if (method === "transform") {
        if (returnedExpressions(arg).some(fallbackFabricates)) record(node, "fabricated-default");
      } else if (method === "default" || method === "catch") {
        // `.default(0)` / `.catch([])` are the idiomatic zod spelling of exactly
        // what this check exists to ban -- and were the largest gap. A function
        // argument (`.catch(() => [])`, including a promise catch) is the same
        // lie, so it is judged by what it returns.
        const returns = returnedExpressions(arg);
        const fabricates = returns.length
          ? returns.some(fabricatesAValue)
          : fabricatesAValue(arg);
        if (fabricates) record(node, "fabricated-default");
      }
    }

    // A cast is the only way to return a value the parse just rejected, so the
    // branch reached on failure is where to look for one.
    if (isFunctionish(node) && node.body && callsSafeParse(node.body)) {
      for (const region of failureRegions(node.body)) {
        for (const ret of ownReturns(region)) {
          if (!ret.expression) continue;
          if (!outerCast(ret.expression)) continue;
          if (fabricatesAValue(unwrap(ret.expression))) record(ret, "cast-after-failed-parse");
        }
      }
    }

    if (inDataTree && ts.isCatchClause(node)) {
      for (const ret of ownReturns(node.block)) {
        if (ret.expression && fabricatesALiteral(ret.expression)) {
          record(ret, "fabricating-catch");
        }
      }
    }

    ts.forEachChild(node, visit);
  };

  visit(source);
  return found;
}

function loadAllowlist(): Allow[] {
  if (!existsSync(ALLOWLIST_PATH)) return [];
  const parsed: unknown = JSON.parse(readFileSync(ALLOWLIST_PATH, "utf8"));
  return Array.isArray(parsed) ? (parsed as Allow[]) : [];
}

/**
 * The baseline matches by file + violation text, not by line: these trees are
 * edited concurrently, and a line-keyed entry broke every time an unrelated
 * edit above it shifted the file. The match is a count-strict multiset -- three
 * baselined `?? []` in a file cover exactly three, so a fourth identical
 * violation still fails. Lines in the file are kept for human navigation only.
 */
type BaselineRow = { file: string; line: number; text: string };

function loadBaseline(path: string): Map<string, number> {
  const counts = new Map<string, number>();
  if (!existsSync(path)) return counts;
  const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
  const rows = Array.isArray(parsed) ? (parsed as BaselineRow[]) : [];
  for (const r of rows) {
    const key = `${r.file}#${r.text}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

const check = process.argv.includes("--check");
const writeBaseline = process.argv.includes("--baseline");
const allow = loadAllowlist();
const exempt = new Set(allow.map((a) => `${a.file}:${a.line}`));
const baseline = writeBaseline ? new Map<string, number>() : loadBaseline(BASELINE_PATH);

const all = ROOTS.filter((r) => existsSync(r))
  .flatMap((r) => tsFiles(r))
  .flatMap(scan);

if (writeBaseline) {
  const rows = all.map((v) => ({ file: v.file, line: v.line, text: v.text }));
  writeFileSync(BASELINE_PATH, `${JSON.stringify(rows, null, 2)}\n`);
  console.log(`schema honesty: baselined ${rows.length} pre-existing default(s) to`);
  console.log(`  ${relative(SITES, BASELINE_PATH)}`);
  console.log(`These are debt, not endorsements. New violations now fail the gate.`);
  process.exit(0);
}

// An exemption whose line no longer holds a violation is the dangerous case,
// not a tidy-up: entries are keyed by file:line with nothing binding them to
// the code they excuse, so an edit that shifts lines silently moves an
// exemption onto a DIFFERENT default -- blessing something nobody reviewed
// while the gate stays green.
const hit = new Set(all.map((v) => `${v.file}:${v.line}`));
const stale = allow.filter((a) => !hit.has(`${a.file}:${a.line}`));
if (stale.length > 0 && !writeBaseline) {
  console.error("schema honesty: exemption(s) no longer point at a violation:");
  for (const a of stale) console.error(`  ${a.file}:${a.line}`);
  console.error(
    "Either the code was fixed -- delete the entry -- or the line moved and the\n" +
      "exemption is now excusing a different default. Re-point it and re-read the why.",
  );
  process.exit(1);
}

const remaining = new Map(baseline);
const violations = all.filter((v) => {
  if (exempt.has(`${v.file}:${v.line}`)) return false;
  const key = `${v.file}#${v.text}`;
  const covered = remaining.get(key) ?? 0;
  if (covered > 0) {
    remaining.set(key, covered - 1);
    return false;
  }
  return true;
});
const baselineTotal = [...baseline.values()].reduce((a, b) => a + b, 0);

if (violations.length === 0) {
  console.log(
    `schema honesty: OK (${exempt.size} exempted, ${baselineTotal} pre-existing untriaged)`,
  );
  process.exit(0);
}

const perRule = new Map<Rule, number>();
for (const v of violations) perRule.set(v.rule, (perRule.get(v.rule) ?? 0) + 1);
const tally = [...perRule].map(([rule, n]) => `${n} ${rule}`).join(", ");

console.error(`schema honesty: ${violations.length} site(s) fabricate a value (${tally})\n`);
for (const v of violations) console.error(`  ${v.file}:${v.line}  [${v.rule}]\n    ${v.text}`);
console.error(
  `\nA default that is not null makes safeParse unable to fail, so a broken` +
    `\nupstream renders as a real measurement. Either let the parse fail and` +
    `\nhandle the absence at the call site, or default to null.` +
    `\n\nA cast after a failed parse and a catch that returns a literal say the` +
    `\nsame thing later: the check ran, rejected the payload, and the value` +
    `\nshipped anyway. Return null, or let the failure reach the caller.` +
    `\n\nIf a fabricated value is genuinely correct here, add it to` +
    `\n  scripts/schema-honesty-allow.json  with a "why" that says what makes` +
    `\nthe fabricated value true.`,
);
process.exit(check ? 1 : 0);
