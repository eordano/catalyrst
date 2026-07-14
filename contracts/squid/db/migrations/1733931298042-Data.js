module.exports = class Data1733931298042 {
    name = 'Data1733931298042'

    async up(db) {
        await db.query(`ALTER TABLE "nft" ADD "search_order_expires_at_normalized" TIMESTAMP WITH TIME ZONE`)
        await db.query(`ALTER TABLE "order" ADD "expires_at_normalized" TIMESTAMP WITH TIME ZONE NOT NULL`)
        await db.query(`CREATE INDEX "IDX_83603c168bc00b20544539fbea" ON "account" ("address") `)
        await db.query(`CREATE INDEX "IDX_3baa214ec3db0ce29708750e3b" ON "nft" ("category") `)
        await db.query(`CREATE INDEX "IDX_e0e405184c1c9253bbe95b6cc7" ON "nft" ("search_order_expires_at_normalized") `)
        await db.query(`CREATE INDEX "IDX_b53fdf02d6f6047c1758ae885a" ON "nft" ("search_is_land") `)
        await db.query(`CREATE INDEX "IDX_4c7d1118621f3ea97740a1d876" ON "nft" ("item_id", "owner_id") `)
        await db.query(`CREATE INDEX "IDX_2485593ed8c9972197aeaf7da6" ON "order" ("expires_at_normalized") `)
    }

    async down(db) {
        await db.query(`ALTER TABLE "nft" DROP COLUMN "search_order_expires_at_normalized"`)
        await db.query(`ALTER TABLE "order" DROP COLUMN "expires_at_normalized"`)
        await db.query(`DROP INDEX "public"."IDX_83603c168bc00b20544539fbea"`)
        await db.query(`DROP INDEX "public"."IDX_3baa214ec3db0ce29708750e3b"`)
        await db.query(`DROP INDEX "public"."IDX_e0e405184c1c9253bbe95b6cc7"`)
        await db.query(`DROP INDEX "public"."IDX_b53fdf02d6f6047c1758ae885a"`)
        await db.query(`DROP INDEX "public"."IDX_4c7d1118621f3ea97740a1d876"`)
        await db.query(`DROP INDEX "public"."IDX_2485593ed8c9972197aeaf7da6"`)
    }
}
