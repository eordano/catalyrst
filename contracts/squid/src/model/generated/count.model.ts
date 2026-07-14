import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_} from "@subsquid/typeorm-store"
import {Network} from "./_network"

@Entity_()
export class Count {
    constructor(props?: Partial<Count>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @IntColumn_({nullable: false})
    orderTotal!: number

    @IntColumn_({nullable: false})
    orderParcel!: number

    @IntColumn_({nullable: false})
    orderEstate!: number

    @IntColumn_({nullable: false})
    orderWearable!: number

    @IntColumn_({nullable: false})
    orderENS!: number

    @IntColumn_({nullable: false})
    parcelTotal!: number

    @IntColumn_({nullable: false})
    estateTotal!: number

    @IntColumn_({nullable: false})
    wearableTotal!: number

    @IntColumn_({nullable: false})
    ensTotal!: number

    @IntColumn_({nullable: false})
    started!: number

    @IntColumn_({nullable: false})
    salesTotal!: number

    @BigIntColumn_({nullable: false})
    salesManaTotal!: bigint

    @BigIntColumn_({nullable: false})
    creatorEarningsManaTotal!: bigint

    @BigIntColumn_({nullable: false})
    daoEarningsManaTotal!: bigint

    @IntColumn_({nullable: false})
    bidTotal!: number

    @IntColumn_({nullable: false})
    collectionTotal!: number

    @IntColumn_({nullable: false})
    itemTotal!: number

    @IntColumn_({nullable: false})
    nftTotal!: number

    @IntColumn_({nullable: false})
    primarySalesTotal!: number

    @BigIntColumn_({nullable: false})
    primarySalesManaTotal!: bigint

    @IntColumn_({nullable: false})
    secondarySalesTotal!: number

    @BigIntColumn_({nullable: false})
    secondarySalesManaTotal!: bigint

    @BigIntColumn_({nullable: false})
    royaltiesManaTotal!: bigint

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
