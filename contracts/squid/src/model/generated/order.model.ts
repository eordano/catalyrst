import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, StringColumn as StringColumn_, ManyToOne as ManyToOne_, Index as Index_, BigIntColumn as BigIntColumn_, DateTimeColumn as DateTimeColumn_} from "@subsquid/typeorm-store"
import {Category} from "./_category"
import {NFT} from "./nft.model"
import {OrderStatus} from "./_orderStatus"
import {Item} from "./item.model"
import {Network} from "./_network"

@Entity_()
export class Order {
    constructor(props?: Partial<Order>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @StringColumn_({nullable: false})
    marketplaceAddress!: string

    @Column_("varchar", {length: 8, nullable: false})
    category!: Category

    @Index_()
    @ManyToOne_(() => NFT, {nullable: true})
    nft!: NFT | undefined | null

    @StringColumn_({nullable: false})
    nftAddress!: string

    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @StringColumn_({nullable: false})
    txHash!: string

    @StringColumn_({nullable: false})
    owner!: string

    @StringColumn_({nullable: true})
    buyer!: string | undefined | null

    @BigIntColumn_({nullable: false})
    price!: bigint

    @Column_("varchar", {length: 11, nullable: false})
    status!: OrderStatus

    @BigIntColumn_({nullable: false})
    blockNumber!: bigint

    @BigIntColumn_({nullable: false})
    expiresAt!: bigint

    @Index_()
    @DateTimeColumn_({nullable: false})
    expiresAtNormalized!: Date

    @BigIntColumn_({nullable: false})
    createdAt!: bigint

    @BigIntColumn_({nullable: false})
    updatedAt!: bigint

    @Index_()
    @ManyToOne_(() => Item, {nullable: true})
    item!: Item | undefined | null

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
