module.exports = class Data1746626335162 {
    name = 'Data1746626335162'

    async up(db) {
        await db.query(`ALTER TABLE "sale" ADD "real_buyer" text NOT NULL`)
        await db.query(`ALTER TABLE "sale" ADD "operation" character varying(11) NOT NULL`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "sale" DROP COLUMN "real_buyer"`)
        await db.query(`ALTER TABLE "sale" DROP COLUMN "operation"`)
    }
}
