module.exports = class Data1737543756081 {
    name = 'Data1737543756081'

    async up(db) {
        await db.query(`ALTER TABLE "bid" DROP COLUMN "status"`)
        await db.query(`ALTER TABLE "bid" ADD "status" character varying(11) NOT NULL`)
        await db.query(`ALTER TABLE "nft" DROP COLUMN "search_order_status"`)
        await db.query(`ALTER TABLE "nft" ADD "search_order_status" character varying(11)`)
        await db.query(`ALTER TABLE "order" DROP COLUMN "status"`)
        await db.query(`ALTER TABLE "order" ADD "status" character varying(11) NOT NULL`)
        await db.query(`CREATE INDEX "IDX_5f8cc4778564d0bd3c4ac3436d" ON "nft" ("search_order_status", "search_order_expires_at", "category") `)
        await db.query(`CREATE INDEX "IDX_d5b8837a62eb6d9c95eb3d2ef2" ON "nft" ("search_order_status", "search_order_expires_at", "network") `)
    }

    async down(db) {
        await db.query(`ALTER TABLE "bid" ADD "status" character varying(9) NOT NULL`)
        await db.query(`ALTER TABLE "bid" DROP COLUMN "status"`)
        await db.query(`CREATE INDEX "IDX_d5b8837a62eb6d9c95eb3d2ef2" ON "nft" ("search_order_status", "search_order_expires_at", "network") `)
        await db.query(`CREATE INDEX "IDX_5f8cc4778564d0bd3c4ac3436d" ON "nft" ("category", "search_order_status", "search_order_expires_at") `)
        await db.query(`ALTER TABLE "nft" ADD "search_order_status" character varying(9)`)
        await db.query(`ALTER TABLE "nft" DROP COLUMN "search_order_status"`)
        await db.query(`ALTER TABLE "order" ADD "status" character varying(9) NOT NULL`)
        await db.query(`ALTER TABLE "order" DROP COLUMN "status"`)
    }
}
