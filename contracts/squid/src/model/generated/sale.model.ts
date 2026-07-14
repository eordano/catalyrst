import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, Index as Index_, StringColumn as StringColumn_, BigIntColumn as BigIntColumn_, ManyToOne as ManyToOne_, BytesColumn as BytesColumn_} from "@subsquid/typeorm-store"
import {SaleType} from "./_saleType"
import {Operation} from "./_operation"
import {NFT} from "./nft.model"
import {Item} from "./item.model"
import {Network} from "./_network"

@Index_(["searchCategory", "network"], {unique: false})
@Entity_()
export class Sale {
    constructor(props?: Partial<Sale>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Column_("varchar", {length: 5, nullable: false})
    type!: SaleType

    @StringColumn_({nullable: false})
    buyer!: string

    @StringColumn_({nullable: false})
    realBuyer!: string

    @Column_("varchar", {length: 11, nullable: false})
    operation!: Operation

    @StringColumn_({nullable: false})
    seller!: string

    @BigIntColumn_({nullable: false})
    price!: bigint

    @Index_()
    @ManyToOne_(() => NFT, {nullable: true})
    nft!: NFT

    @Index_()
    @BigIntColumn_({nullable: false})
    timestamp!: bigint

    @StringColumn_({nullable: false})
    txHash!: string

    @BigIntColumn_({nullable: false})
    searchTokenId!: bigint

    @StringColumn_({nullable: false})
    searchContractAddress!: string

    @StringColumn_({nullable: false})
    searchCategory!: string

    @BytesColumn_({nullable: true})
    beneficiary!: Uint8Array | undefined | null

    @BigIntColumn_({nullable: true})
    feesCollectorCut!: bigint | undefined | null

    @BytesColumn_({nullable: true})
    feesCollector!: Uint8Array | undefined | null

    @BigIntColumn_({nullable: true})
    royaltiesCut!: bigint | undefined | null

    @BytesColumn_({nullable: true})
    royaltiesCollector!: Uint8Array | undefined | null

    @Index_()
    @ManyToOne_(() => Item, {nullable: true})
    item!: Item | undefined | null

    @BigIntColumn_({nullable: true})
    searchItemId!: bigint | undefined | null

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
