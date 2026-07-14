module.exports = class Data1734457709893 {
  name = "Data1734457709893";

  async up(db) {
    await db.query(
      `CREATE INDEX "IDX_1f3ec6150afbb8a3fd75fae814" ON "estate" ("size") `
    );
    await db.query(
      `CREATE INDEX "IDX_9ddbd0267ddb9c59621775f94e" ON "item" ("collection_id", "blockchain_id") `
    );
    await db.query(
      `CREATE INDEX "IDX_0fca1a8c5d9399d9a9a52e26f7" ON "nft" ("contract_address", "token_id") `
    );
  }

  async down(db) {
    await db.query(`DROP INDEX "public"."IDX_1f3ec6150afbb8a3fd75fae814"`);
    await db.query(`DROP INDEX "public"."IDX_9ddbd0267ddb9c59621775f94e"`);
    await db.query(`DROP INDEX "public"."IDX_0fca1a8c5d9399d9a9a52e26f7"`);
  }
};
