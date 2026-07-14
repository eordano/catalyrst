module.exports = class Data1764159055451 {
    name = 'Data1764159055451'

    async up(db) {
        await db.query(`ALTER TABLE "collection" ADD "used_credits" boolean NOT NULL`)
        await db.query(`ALTER TABLE "collection" ADD "credit_value" numeric`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "collection" DROP COLUMN "used_credits"`)
        await db.query(`ALTER TABLE "collection" DROP COLUMN "credit_value"`)
    }
}
