import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, StringColumn as StringColumn_, ManyToOne as ManyToOne_, Index as Index_, BigIntColumn as BigIntColumn_, BytesColumn as BytesColumn_} from "@subsquid/typeorm-store"
import {Category} from "./_category"
import {NFT} from "./nft.model"
import {OrderStatus} from "./_orderStatus"
import {Network} from "./_network"

@Entity_()
export class Bid {
    constructor(props?: Partial<Bid>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @StringColumn_({nullable: false})
    bidAddress!: string

    @Column_("varchar", {length: 8, nullable: false})
    category!: Category

    @Index_()
    @ManyToOne_(() => NFT, {nullable: true})
    nft!: NFT | undefined | null

    @StringColumn_({nullable: false})
    nftAddress!: string

    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @BytesColumn_({nullable: true})
    bidder!: Uint8Array | undefined | null

    @BytesColumn_({nullable: true})
    seller!: Uint8Array | undefined | null

    @BigIntColumn_({nullable: false})
    price!: bigint

    @BytesColumn_({nullable: true})
    fingerprint!: Uint8Array | undefined | null

    @Column_("varchar", {length: 11, nullable: false})
    status!: OrderStatus

    @StringColumn_({nullable: false})
    blockchainId!: string

    @BigIntColumn_({nullable: false})
    blockNumber!: bigint

    @BigIntColumn_({nullable: false})
    expiresAt!: bigint

    @BigIntColumn_({nullable: false})
    createdAt!: bigint

    @BigIntColumn_({nullable: false})
    updatedAt!: bigint

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
