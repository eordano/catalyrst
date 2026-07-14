module.exports = class Data1735669466432 {
    name = 'Data1735669466432'

    async up(db) {
        await db.query(`ALTER TABLE "nft" ADD "owner_address" text NOT NULL`)
        await db.query(`CREATE INDEX "IDX_26e756121a20d1cc3e4d738279" ON "nft" ("owner_address") `)
    }

    async down(db) {
        await db.query(`ALTER TABLE "nft" DROP COLUMN "owner_address"`)
        await db.query(`DROP INDEX "public"."IDX_26e756121a20d1cc3e4d738279"`)
    }
}
