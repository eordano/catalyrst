module.exports = class Data1720782180337 {
    name = 'Data1720782180337'

    async up(db) {
        await db.query(`ALTER TABLE "nft" DROP COLUMN "contract_address"`)
        await db.query(`ALTER TABLE "nft" ADD "contract_address" text NOT NULL`)
        await db.query(`ALTER TABLE "sale" DROP COLUMN "search_contract_address"`)
        await db.query(`ALTER TABLE "sale" ADD "search_contract_address" text NOT NULL`)
    }

    async down(db) {
        await db.query(`ALTER TABLE "nft" ADD "contract_address" bytea NOT NULL`)
        await db.query(`ALTER TABLE "nft" DROP COLUMN "contract_address"`)
        await db.query(`ALTER TABLE "sale" ADD "search_contract_address" bytea NOT NULL`)
        await db.query(`ALTER TABLE "sale" DROP COLUMN "search_contract_address"`)
    }
}
