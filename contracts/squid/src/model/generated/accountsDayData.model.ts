import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_, StringColumn as StringColumn_} from "@subsquid/typeorm-store"
import {Network} from "./_network"

@Entity_()
export class AccountsDayData {
    constructor(props?: Partial<AccountsDayData>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @IntColumn_({nullable: false})
    date!: number

    @IntColumn_({nullable: false})
    sales!: number

    @IntColumn_({nullable: false})
    purchases!: number

    @BigIntColumn_({nullable: false})
    earned!: bigint

    @BigIntColumn_({nullable: false})
    spent!: bigint

    @StringColumn_({array: true, nullable: false})
    uniqueCollectionsSales!: (string)[]

    @StringColumn_({array: true, nullable: false})
    uniqueCollectors!: (string)[]

    @IntColumn_({nullable: false})
    uniqueCollectorsTotal!: number

    @StringColumn_({array: true, nullable: false})
    uniqueAndMythicItems!: (string)[]

    @IntColumn_({nullable: false})
    uniqueAndMythicItemsTotal!: number

    @StringColumn_({array: true, nullable: false})
    creatorsSupported!: (string)[]

    @IntColumn_({nullable: false})
    creatorsSupportedTotal!: number

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
