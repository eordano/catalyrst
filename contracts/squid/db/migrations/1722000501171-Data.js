module.exports = class Data1722000501171 {
    name = 'Data1722000501171'

    async up(db) {
        await db.query(`ALTER TABLE "mint" ADD "network" character varying(8) NOT NULL`)
        await db.query(`CREATE INDEX "IDX_7e215df412b248db3731737290" ON "nft" ("token_id") `)
    }

    async down(db) {
        await db.query(`ALTER TABLE "mint" DROP COLUMN "network"`)
        await db.query(`DROP INDEX "public"."IDX_7e215df412b248db3731737290"`)
    }
}
