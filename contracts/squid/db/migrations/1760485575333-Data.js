module.exports = class Data1760485575333 {
    name = 'Data1760485575333'

    async up(db) {
        await db.query(`ALTER TABLE "emote" ADD "outcome_type" character varying(16)`)
        await db.query(`ALTER TABLE "item" ADD "search_emote_outcome_type" character varying(16)`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "emote" DROP COLUMN "outcome_type"`)
        await db.query(`ALTER TABLE "item" DROP COLUMN "search_emote_outcome_type"`)
    }
}
