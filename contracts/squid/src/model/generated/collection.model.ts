import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, OneToMany as OneToMany_, StringColumn as StringColumn_, BooleanColumn as BooleanColumn_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_} from "@subsquid/typeorm-store"
import {Item} from "./item.model"
import {Network} from "./_network"

@Entity_()
export class Collection {
    constructor(props?: Partial<Collection>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @OneToMany_(() => Item, e => e.collection)
    items!: Item[]

    @StringColumn_({nullable: false})
    owner!: string

    @StringColumn_({nullable: false})
    creator!: string

    @StringColumn_({nullable: false})
    name!: string

    @StringColumn_({nullable: false})
    symbol!: string

    @BooleanColumn_({nullable: true})
    isCompleted!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    isApproved!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    isEditable!: boolean | undefined | null

    @StringColumn_({array: true, nullable: false})
    minters!: (string)[]

    @StringColumn_({array: true, nullable: false})
    managers!: (string)[]

    @StringColumn_({nullable: false})
    urn!: string

    @IntColumn_({nullable: false})
    itemsCount!: number

    @BigIntColumn_({nullable: false})
    createdAt!: bigint

    @BigIntColumn_({nullable: false})
    updatedAt!: bigint

    @BigIntColumn_({nullable: false})
    reviewedAt!: bigint

    @BigIntColumn_({nullable: true})
    firstListedAt!: bigint | undefined | null

    @BooleanColumn_({nullable: false})
    searchIsStoreMinter!: boolean

    @BooleanColumn_({nullable: false})
    searchIsMarketplaceV3Minter!: boolean

    @StringColumn_({nullable: false})
    searchText!: string

    @StringColumn_({nullable: false})
    baseURI!: string

    @BigIntColumn_({nullable: false})
    chainId!: bigint

    @BooleanColumn_({nullable: false})
    usedCredits!: boolean

    @BigIntColumn_({nullable: true})
    creditValue!: bigint | undefined | null

    @StringColumn_({nullable: false})
    txHash!: string

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
