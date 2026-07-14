import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, BytesColumn as BytesColumn_, ManyToOne as ManyToOne_, Index as Index_, BooleanColumn as BooleanColumn_, BigIntColumn as BigIntColumn_} from "@subsquid/typeorm-store"
import {Account} from "./account.model"
import {Collection} from "./collection.model"
import {Item} from "./item.model"

@Entity_()
export class Curation {
    constructor(props?: Partial<Curation>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @BytesColumn_({nullable: false})
    txHash!: Uint8Array

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    curator!: Account

    @Index_()
    @ManyToOne_(() => Collection, {nullable: true})
    collection!: Collection

    @Index_()
    @ManyToOne_(() => Item, {nullable: true})
    item!: Item | undefined | null

    @BooleanColumn_({nullable: false})
    isApproved!: boolean

    @BigIntColumn_({nullable: false})
    timestamp!: bigint
}
