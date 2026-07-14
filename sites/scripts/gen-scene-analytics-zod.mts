#!/usr/bin/env node
// Generates the creators-data wire Zod schema + snake->camel mappers from the
// canonical camelCase model in catalyrst/ui3/src/creatorhub/lib/scene-analytics.ts.
//
// The creators-data server (a sibling repo, plain JS) serves snake_cased JSON
// of exactly that model, so the wire schema is mechanically derivable: every
// wire key is snake_case(model key), `T | null` fields are nullable with a
// null default, and arrays are required. Rather than hand-maintaining three
// parallel artifacts (model types, wire schema, transform), this script emits
// the latter two from the first. The emitted mappers are typed against the
// @ui model imports, so if the model changes without a re-run, `tsc` fails --
// same drift-surfacing principle as the ts-rs pipeline in gen-ts-types.sh.
//
//   node scripts/gen-scene-analytics-zod.mts            # rewrite the .gen.ts
//   node scripts/gen-scene-analytics-zod.mts --stdout   # print (drift test)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript6";

const here = dirname(fileURLToPath(import.meta.url));
const SRC = resolve(
  here,
  "../../ui3/src/creatorhub/lib/scene-analytics.ts",
);
const OUT = resolve(
  here,
  "../packages/data/src/lib/catalyst/creator-hub/scene-analytics.gen.ts",
);

// Dependency order; each entry becomes a Wire<Name>Schema + to<Name> mapper
// (enums become a plain schema, no mapper).
const TYPES = [
  "SceneType",
  "SceneMetricsWindow",
  "SceneRetention",
  "SceneRetentionPoint",
  "SceneDailyStats",
  "SceneStats",
  "CreatorScenesStats",
];

type FieldCodegen = { schema: string; map: (a: string) => string };
type Field = { name: string; wire: string } & FieldCodegen;

function snake(name: string): string {
  return name.replace(
    /([A-Z])/g,
    (_, c: string, off: number) => (off > 0 ? "_" : "") + c.toLowerCase(),
  );
}

const source = ts.createSourceFile(
  SRC,
  readFileSync(SRC, "utf8"),
  ts.ScriptTarget.Latest,
  true,
);

const aliases = new Map<string, ts.TypeNode>();
source.forEachChild((node) => {
  if (ts.isTypeAliasDeclaration(node)) aliases.set(node.name.text, node.type);
});

for (const name of TYPES) {
  if (!aliases.has(name)) {
    console.error(`type ${name} not found in ${SRC}`);
    process.exit(1);
  }
}

function isNullLiteral(t: ts.TypeNode): boolean {
  return (
    ts.isLiteralTypeNode(t) && t.literal.kind === ts.SyntaxKind.NullKeyword
  );
}

function stringEnumMembers(t: ts.TypeNode): string[] | null {
  if (!ts.isUnionTypeNode(t)) return null;
  const values: string[] = [];
  for (const member of t.types) {
    if (ts.isLiteralTypeNode(member) && ts.isStringLiteral(member.literal)) {
      values.push(member.literal.text);
    } else {
      return null;
    }
  }
  return values;
}

// -> { schema: string, map: (accessor) => string }
function fieldCodegen(t: ts.TypeNode): FieldCodegen {
  if (t.kind === ts.SyntaxKind.StringKeyword) {
    return { schema: "z.string()", map: (a) => a };
  }
  if (t.kind === ts.SyntaxKind.NumberKeyword) {
    return { schema: "z.number()", map: (a) => a };
  }
  if (ts.isUnionTypeNode(t)) {
    const rest = t.types.filter((m) => !isNullLiteral(m));
    if (rest.length !== t.types.length && rest.length === 1) {
      const inner = fieldCodegen(rest[0]!);
      return {
        schema: `${inner.schema}.nullable().default(null)`,
        map: inner.map,
      };
    }
    const enumValues = stringEnumMembers(t);
    if (enumValues) {
      return {
        schema: `z.enum([${enumValues.map((v) => JSON.stringify(v)).join(", ")}])`,
        map: (a) => a,
      };
    }
  }
  if (ts.isArrayTypeNode(t)) {
    const inner = fieldCodegen(t.elementType);
    const identity = inner.map("x") === "x";
    // No `.default([])`. The model's arrays are the measurements themselves --
    // a scene's daily rows, its retention series, a creator's scene list -- so
    // an absent key is a read that did not happen, and an empty array renders
    // as "this scene had no visitors" or "you published no scenes".
    return {
      schema: `z.array(${inner.schema})`,
      map: identity ? (a: string) => a : (a: string) => `${a}.map((x) => ${inner.map("x")})`,
    };
  }
  if (ts.isTypeReferenceNode(t)) {
    const ref = t.typeName.getText(source);
    if (!TYPES.includes(ref)) {
      console.error(`unsupported type reference: ${ref}`);
      process.exit(1);
    }
    const isEnum = stringEnumMembers(aliases.get(ref)!) !== null;
    return {
      schema: `Wire${ref}Schema`,
      map: isEnum ? (a: string) => a : (a: string) => `to${ref}(${a})`,
    };
  }
  if (ts.isTypeLiteralNode(t)) {
    const fields = objectFields(t);
    return {
      schema: objectSchema(fields, "  "),
      map: (a: string) => objectMapper(fields, a, "  "),
    };
  }
  console.error(`unsupported type node kind: ${ts.SyntaxKind[t.kind]}`);
  process.exit(1);
}

function objectFields(typeLiteral: ts.TypeLiteralNode): Field[] {
  return typeLiteral.members.map((member) => {
    if (!ts.isPropertySignature(member) || !member.type) {
      console.error("unsupported member in type literal");
      process.exit(1);
    }
    const name = member.name.getText(source);
    return { name, wire: snake(name), ...fieldCodegen(member.type) };
  });
}

function indentBlock(text: string, pad: string): string {
  return text
    .split("\n")
    .map((line, i) => (i === 0 ? line : pad + line))
    .join("\n");
}

function objectSchema(fields: Field[], pad = ""): string {
  const lines = fields.map(
    (f) => `${pad}  ${f.wire}: ${indentBlock(f.schema, `${pad}  `)},`,
  );
  return `z.object({\n${lines.join("\n")}\n${pad}})`;
}

function objectMapper(fields: Field[], accessor: string, pad = ""): string {
  const lines = fields.map(
    (f) =>
      `${pad}  ${f.name}: ${indentBlock(f.map(`${accessor}.${f.wire}`), `${pad}  `)},`,
  );
  return `{\n${lines.join("\n")}\n${pad}}`;
}

const parts: string[] = [];
parts.push(`// Generated by scripts/gen-scene-analytics-zod.mts -- do not edit.
// Source of truth: catalyrst/ui3/src/creatorhub/lib/scene-analytics.ts (camelCase model).
// Wire = snake_cased model, nullable fields defaulting to null, arrays to [].
// Regenerate: npm run gen:scene-analytics (drift-gated by scene-analytics.gen.test.ts).
import { z } from "zod";
import type {
${TYPES.filter((n) => stringEnumMembers(aliases.get(n)!) === null)
  .map((n) => `  ${n},`)
  .join("\n")}
} from "@ui/creatorhub/lib/scene-analytics";
`);

for (const name of TYPES) {
  const t = aliases.get(name)!;
  const enumValues = stringEnumMembers(t);
  if (enumValues) {
    parts.push(
      `const Wire${name}Schema = z.enum([${enumValues
        .map((v) => JSON.stringify(v))
        .join(", ")}]);\n`,
    );
    continue;
  }
  if (!ts.isTypeLiteralNode(t)) {
    console.error(`type ${name} is not an object type`);
    process.exit(1);
  }
  const fields = objectFields(t);
  const exported = name === "CreatorScenesStats" ? "export " : "";
  parts.push(`${exported}const Wire${name}Schema = ${objectSchema(fields)};\n`);
  parts.push(
    `function to${name}(w: z.infer<typeof Wire${name}Schema>): ${name} {\n  return ${indentBlock(
      objectMapper(fields, "w"),
      "  ",
    )};\n}\n`,
  );
}

parts.push(`export function parseCreatorScenesStats(
  raw: unknown,
): CreatorScenesStats {
  return toCreatorScenesStats(WireCreatorScenesStatsSchema.parse(raw));
}
`);

const output = parts.join("\n");

if (process.argv.includes("--stdout")) {
  process.stdout.write(output);
} else {
  writeFileSync(OUT, output);
  console.log(`generated ${relative(resolve(here, ".."), OUT)}`);
}
