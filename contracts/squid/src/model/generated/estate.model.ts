import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, BigIntColumn as BigIntColumn_, ManyToOne as ManyToOne_, Index as Index_, OneToMany as OneToMany_, IntColumn as IntColumn_, StringColumn as StringColumn_, OneToOne as OneToOne_} from "@subsquid/typeorm-store"
import {Account} from "./account.model"
import {Parcel} from "./parcel.model"
import {Data} from "./data.model"
import {NFT} from "./nft.model"

@Entity_()
export class Estate {
    constructor(props?: Partial<Estate>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    owner!: Account

    @OneToMany_(() => Parcel, e => e.estate)
    parcels!: Parcel[]

    @IntColumn_({array: true, nullable: true})
    parcelDistances!: (number)[] | undefined | null

    @IntColumn_({nullable: true})
    adjacentToRoadCount!: number | undefined | null

    @Index_()
    @IntColumn_({nullable: true})
    size!: number | undefined | null

    @Index_()
    @ManyToOne_(() => Data, {nullable: true})
    data!: Data | undefined | null

    @StringColumn_({nullable: true})
    rawData!: string | undefined | null

    @OneToOne_(() => NFT, e => e.estate)
    nft!: NFT | undefined | null
}
