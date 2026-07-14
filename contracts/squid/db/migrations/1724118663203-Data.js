module.exports = class Data1724118663203 {
    name = 'Data1724118663203'

    async up(db) {
        await db.query(`CREATE INDEX "IDX_2c8ca873555fc156848199919f" ON "nft" ("created_at") `)
        await db.query(`CREATE INDEX "IDX_645ec1a1710c449fa4e9d241e9" ON "nft" ("search_order_expires_at") `)
        await db.query(`CREATE INDEX "IDX_4d213d73326e54427a5c9bdddf" ON "nft" ("search_parcel_is_in_bounds") `)
        await db.query(`CREATE INDEX "IDX_8ac00a610840894296c6f32fd2" ON "sale" ("timestamp") `)
        await db.query(`CREATE INDEX "IDX_a91d7a7aa55af7d57ef4d17912" ON "sale" ("search_category", "network") `)
    }

    async down(db) {
        await db.query(`DROP INDEX "public"."IDX_2c8ca873555fc156848199919f"`)
        await db.query(`DROP INDEX "public"."IDX_645ec1a1710c449fa4e9d241e9"`)
        await db.query(`DROP INDEX "public"."IDX_4d213d73326e54427a5c9bdddf"`)
        await db.query(`DROP INDEX "public"."IDX_8ac00a610840894296c6f32fd2"`)
        await db.query(`DROP INDEX "public"."IDX_a91d7a7aa55af7d57ef4d17912"`)
    }
}
