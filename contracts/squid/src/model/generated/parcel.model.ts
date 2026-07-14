import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, BigIntColumn as BigIntColumn_, ManyToOne as ManyToOne_, Index as Index_, StringColumn as StringColumn_, OneToOne as OneToOne_} from "@subsquid/typeorm-store"
import {Account} from "./account.model"
import {Estate} from "./estate.model"
import {Data} from "./data.model"
import {NFT} from "./nft.model"

@Entity_()
export class Parcel {
    constructor(props?: Partial<Parcel>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    owner!: Account

    @BigIntColumn_({nullable: false})
    x!: bigint

    @BigIntColumn_({nullable: false})
    y!: bigint

    @Index_()
    @ManyToOne_(() => Estate, {nullable: true})
    estate!: Estate | undefined | null

    @Index_()
    @ManyToOne_(() => Data, {nullable: true})
    data!: Data | undefined | null

    @StringColumn_({nullable: true})
    rawData!: string | undefined | null

    @OneToOne_(() => NFT, e => e.parcel)
    nft!: NFT | undefined | null
}
