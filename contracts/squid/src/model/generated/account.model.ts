import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, StringColumn as StringColumn_, Index as Index_, OneToMany as OneToMany_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_, BooleanColumn as BooleanColumn_} from "@subsquid/typeorm-store"
import {NFT} from "./nft.model"
import {Network} from "./_network"

@Entity_()
export class Account {
    constructor(props?: Partial<Account>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Index_()
    @StringColumn_({nullable: false})
    address!: string

    @OneToMany_(() => NFT, e => e.owner)
    nfts!: NFT[]

    @IntColumn_({nullable: false})
    sales!: number

    @IntColumn_({nullable: false})
    purchases!: number

    @BigIntColumn_({nullable: false})
    spent!: bigint

    @BigIntColumn_({nullable: false})
    earned!: bigint

    @BooleanColumn_({nullable: true})
    isCommitteeMember!: boolean | undefined | null

    @IntColumn_({nullable: true})
    totalCurations!: number | undefined | null

    @IntColumn_({nullable: false})
    primarySales!: number

    @BigIntColumn_({nullable: false})
    primarySalesEarned!: bigint

    @BigIntColumn_({nullable: false})
    royalties!: bigint

    @StringColumn_({array: true, nullable: false})
    uniqueAndMythicItems!: (string)[]

    @IntColumn_({nullable: false})
    uniqueAndMythicItemsTotal!: number

    @IntColumn_({nullable: false})
    collections!: number

    @StringColumn_({array: true, nullable: false})
    creatorsSupported!: (string)[]

    @IntColumn_({nullable: false})
    creatorsSupportedTotal!: number

    @StringColumn_({array: true, nullable: false})
    uniqueCollectors!: (string)[]

    @IntColumn_({nullable: false})
    uniqueCollectorsTotal!: number

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
