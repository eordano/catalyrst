import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, StringColumn as StringColumn_, BigIntColumn as BigIntColumn_} from "@subsquid/typeorm-store"
import {Currency} from "./_currency"

@Entity_()
export class Rarity {
    constructor(props?: Partial<Rarity>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @StringColumn_({nullable: false})
    name!: string

    @BigIntColumn_({nullable: false})
    maxSupply!: bigint

    @BigIntColumn_({nullable: false})
    price!: bigint

    @Column_("varchar", {length: 4, nullable: false})
    currency!: Currency
}
