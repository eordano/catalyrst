module.exports = class Data1764165333455 {
    name = 'Data1764165333455'

    async up(db) {
        await db.query(`ALTER TABLE "collection" ADD "tx_hash" text NOT NULL`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "collection" DROP COLUMN "tx_hash"`)
    }
}
