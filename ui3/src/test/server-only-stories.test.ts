import { describe, expect, it } from "vitest";
import { serverOnlyStoryModules } from "../../.storybook/server-only-stories";

function transform(source: string, id = "/repo/sites/packages/data/db.server.ts") {
  const hook = serverOnlyStoryModules().transform;
  if (typeof hook !== "function") throw new Error("Expected transform hook");
  return hook.call({} as never, source, id);
}

describe("browser story server boundary", () => {
  it("removes server imports and initializers while refusing server calls", async () => {
    const result = await transform(`
      import { Pool } from 'pg';
      const secret = process.env.DATABASE_URL;
      const pool = new Pool({ connectionString: secret });
      export async function query() { return pool.query('SELECT 1'); }
      export class DatabaseError extends Error {}
      export type RecordId = string;
    `);
    expect(result).toBeTruthy();
    const code = typeof result === "string" ? result : result!.code;
    if (typeof code !== "string") throw new Error("Expected generated module");
    expect(code).not.toContain("DATABASE_URL");
    expect(code).not.toContain("from 'pg'");
    expect(code).not.toContain("RecordId");
    const module = new Function(code.replaceAll("export function", "function") + ";return {query, DatabaseError};")();
    expect(() => module.query()).toThrow("supply route loaderData");
    expect(() => new module.DatabaseError()).toThrow("supply route loaderData");
  });

  it("leaves browser modules untouched", async () => {
    expect(await transform("export const answer = 42", "/repo/sites/packages/data/client.ts")).toBeUndefined();
  });

  it("fails explicitly on server export patterns it cannot safely represent", () => {
    expect(() => transform("export * from './db.server'" )).toThrow("Explicit server exports");
  });
});
