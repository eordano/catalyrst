import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, ManyToOne as ManyToOne_, Index as Index_, StringColumn as StringColumn_, BigIntColumn as BigIntColumn_, BooleanColumn as BooleanColumn_} from "@subsquid/typeorm-store"
import {Item} from "./item.model"
import {NFT} from "./nft.model"
import {Network} from "./_network"

@Entity_()
export class Mint {
    constructor(props?: Partial<Mint>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Index_()
    @ManyToOne_(() => Item, {nullable: true})
    item!: Item

    @Index_()
    @ManyToOne_(() => NFT, {nullable: true})
    nft!: NFT

    @StringColumn_({nullable: false})
    creator!: string

    @StringColumn_({nullable: false})
    beneficiary!: string

    @StringColumn_({nullable: false})
    minter!: string

    @BigIntColumn_({nullable: false})
    timestamp!: bigint

    @BigIntColumn_({nullable: true})
    searchPrimarySalePrice!: bigint | undefined | null

    @StringColumn_({nullable: false})
    searchContractAddress!: string

    @BigIntColumn_({nullable: false})
    searchItemId!: bigint

    @BigIntColumn_({nullable: false})
    searchTokenId!: bigint

    @BigIntColumn_({nullable: true})
    searchIssuedId!: bigint | undefined | null

    @BooleanColumn_({nullable: false})
    searchIsStoreMinter!: boolean

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
