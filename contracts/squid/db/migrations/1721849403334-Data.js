module.exports = class Data1721849403334 {
    name = 'Data1721849403334'

    async up(db) {
        await db.query(`ALTER TABLE "metadata" ADD "network" character varying(8) NOT NULL`)
        await db.query(`ALTER TABLE "item" ADD "network" character varying(8) NOT NULL`)
        await db.query(`ALTER TABLE "collection" ADD "network" character varying(8) NOT NULL`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "metadata" DROP COLUMN "network"`)
        await db.query(`ALTER TABLE "item" DROP COLUMN "network"`)
        await db.query(`ALTER TABLE "collection" DROP COLUMN "network"`)
    }
}
