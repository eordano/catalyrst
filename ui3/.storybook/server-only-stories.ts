import { babelParse } from "storybook/internal/csf-tools";
import type { Plugin } from "vite";

export function serverOnlyStoryModules(): Plugin {
  return {
    name: "storybook-server-only-boundary",
    enforce: "pre",
    transform(source, id) {
      const file = id.split("?")[0] ?? id;
      if (!file.includes("/sites/packages/") || !/\.server\.[cm]?[jt]sx?$/.test(file)) return;
      const names = new Set<string>();
      let hasDefault = false;
      for (const statement of babelParse(source).program.body) {
        if (statement.type === "ExportDefaultDeclaration") { hasDefault = true; continue; }
        if (statement.type === "ExportAllDeclaration") {
          throw new Error(`Explicit server exports required for story boundary: ${file}`);
        }
        if (statement.type !== "ExportNamedDeclaration" || statement.exportKind === "type") continue;
        for (const item of statement.specifiers) {
          if (item.type === "ExportSpecifier" && item.exportKind !== "type") {
            names.add(item.exported.type === "Identifier" ? item.exported.name : item.exported.value);
          }
        }
        const declaration = statement.declaration;
        if (declaration?.type === "VariableDeclaration") {
          for (const item of declaration.declarations) {
            if (item.id.type !== "Identifier") throw new Error(`Named server exports required: ${file}`);
            names.add(item.id.name);
          }
        } else if ((declaration?.type === "FunctionDeclaration" || declaration?.type === "ClassDeclaration" || declaration?.type === "TSEnumDeclaration") && declaration.id) {
          names.add(declaration.id.name);
        }
      }
      const body = `throw new Error(${JSON.stringify("Server-only code cannot run in a browser story; supply route loaderData instead: " + file)});`;
      return {
        code: [...names].map(name => `export function ${name}() { ${body} }`).join("\n") +
          (hasDefault ? `\nexport default function() { ${body} }` : ""),
        map: null,
      };
    },
  };
}
