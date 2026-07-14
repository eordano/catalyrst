module.exports = class Data1758208597059 {
    name = 'Data1758208597059'

    async up(db) {
        await db.query(`ALTER TABLE "item" ADD "search_is_marketplace_v3_minter" boolean NOT NULL`)
        await db.query(`ALTER TABLE "collection" ADD "search_is_marketplace_v3_minter" boolean NOT NULL`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "item" DROP COLUMN "search_is_marketplace_v3_minter"`)
        await db.query(`ALTER TABLE "collection" DROP COLUMN "search_is_marketplace_v3_minter"`)
    }
}
