module.exports = class Data1733097600000 {
  name = "Data1733097600000";

  async up(db) {
    await db.query(`ALTER TABLE "ens" ADD "order_hash" text`);
    await db.query(`CREATE INDEX "IDX_ens_order_hash" ON "ens" ("order_hash")`);

    await db.query(`CREATE TABLE "squid_router_order" (
            "id" character varying NOT NULL,
            "order_hash" text NOT NULL,
            "credit_ids" text array NOT NULL,
            "total_credits_used" numeric NOT NULL,
            "tx_hash" text NOT NULL,
            "block_number" numeric NOT NULL,
            "timestamp" numeric NOT NULL,
            "network" character varying(8) NOT NULL,
            CONSTRAINT "PK_squid_router_order" PRIMARY KEY ("id")
        )`);
    await db.query(
      `CREATE INDEX "IDX_squid_router_order_order_hash" ON "squid_router_order" ("order_hash")`
    );
    await db.query(
      `CREATE INDEX "IDX_squid_router_order_tx_hash" ON "squid_router_order" ("tx_hash")`
    );
  }

  async down(db) {
    await db.query(`DROP INDEX "IDX_squid_router_order_tx_hash"`);
    await db.query(`DROP INDEX "IDX_squid_router_order_order_hash"`);
    await db.query(`DROP TABLE "squid_router_order"`);
    await db.query(`DROP INDEX "IDX_ens_order_hash"`);
    await db.query(`ALTER TABLE "ens" DROP COLUMN "order_hash"`);
  }
};
