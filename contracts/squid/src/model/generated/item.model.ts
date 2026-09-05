import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, Index as Index_, ManyToOne as ManyToOne_, BigIntColumn as BigIntColumn_, StringColumn as StringColumn_, OneToMany as OneToMany_, IntColumn as IntColumn_, BooleanColumn as BooleanColumn_} from "@subsquid/typeorm-store"
import {Collection} from "./collection.model"
import {ItemType} from "./_itemType"
import {Metadata} from "./metadata.model"
import {NFT} from "./nft.model"
import {WearableCategory} from "./_wearableCategory"
import {WearableBodyShape} from "./_wearableBodyShape"
import {EmoteCategory} from "./_emoteCategory"
import {EmoteOutcomeType} from "./_emoteOutcomeType"
import {Network} from "./_network"

@Index_(["collection", "blockchainId"], {unique: false})
@Entity_()
export class Item {
    constructor(props?: Partial<Item>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @ManyToOne_(() => Collection, {nullable: true})
    collection!: Collection

    @BigIntColumn_({nullable: false})
    blockchainId!: bigint

    @StringColumn_({nullable: false})
    creator!: string

    @Column_("varchar", {length: 17, nullable: false})
    itemType!: ItemType

    @BigIntColumn_({nullable: false})
    totalSupply!: bigint

    @BigIntColumn_({nullable: false})
    maxSupply!: bigint

    @StringColumn_({nullable: false})
    rarity!: string

    @BigIntColumn_({nullable: false})
    creationFee!: bigint

    @BigIntColumn_({nullable: false})
    available!: bigint

    @BigIntColumn_({nullable: false})
    price!: bigint

    @StringColumn_({nullable: false})
    beneficiary!: string

    @StringColumn_({nullable: true})
    contentHash!: string | undefined | null

    @StringColumn_({nullable: false})
    uri!: string

    @StringColumn_({nullable: true})
    image!: string | undefined | null

    @StringColumn_({array: true, nullable: false})
    minters!: (string)[]

    @StringColumn_({array: true, nullable: false})
    managers!: (string)[]

    @Index_()
    @ManyToOne_(() => Metadata, {nullable: true})
    metadata!: Metadata | undefined | null

    @StringColumn_({nullable: false})
    rawMetadata!: string

    @StringColumn_({nullable: false})
    urn!: string

    @OneToMany_(() => NFT, e => e.item)
    nfts!: NFT[]

    @BigIntColumn_({nullable: false})
    createdAt!: bigint

    @BigIntColumn_({nullable: false})
    updatedAt!: bigint

    /**
     * Last time the item was reviewed
     */
    @BigIntColumn_({nullable: false})
    reviewedAt!: bigint

    /**
     * Last time the Item was sold
     */
    @BigIntColumn_({nullable: true})
    soldAt!: bigint | undefined | null

    /**
     * First time the Item was listed
     */
    @BigIntColumn_({nullable: true})
    firstListedAt!: bigint | undefined | null

    @IntColumn_({nullable: false})
    sales!: number

    @BigIntColumn_({nullable: false})
    volume!: bigint

    @StringColumn_({nullable: true})
    searchText!: string | undefined | null

    @StringColumn_({nullable: true})
    searchItemType!: string | undefined | null

    @BooleanColumn_({nullable: true})
    searchIsCollectionApproved!: boolean | undefined | null

    @BooleanColumn_({nullable: false})
    searchIsStoreMinter!: boolean

    @BooleanColumn_({nullable: false})
    searchIsMarketplaceV3Minter!: boolean

    @BooleanColumn_({nullable: true})
    searchIsWearableHead!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    searchIsWearableAccessory!: boolean | undefined | null

    @Column_("varchar", {length: 11, nullable: true})
    searchWearableCategory!: WearableCategory | undefined | null

    @StringColumn_({nullable: true})
    searchWearableRarity!: string | undefined | null

    @Column_("varchar", {length: 10, array: true, nullable: true})
    searchWearableBodyShapes!: (WearableBodyShape)[] | undefined | null

    @Column_("varchar", {length: 13, nullable: true})
    searchEmoteCategory!: EmoteCategory | undefined | null

    @BooleanColumn_({nullable: true})
    searchEmoteLoop!: boolean | undefined | null

    @StringColumn_({nullable: true})
    searchEmoteRarity!: string | undefined | null

    @Column_("varchar", {length: 10, array: true, nullable: true})
    searchEmoteBodyShapes!: (WearableBodyShape)[] | undefined | null

    @BooleanColumn_({nullable: true})
    searchEmoteHasSound!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    searchEmoteHasGeometry!: boolean | undefined | null

    @Column_("varchar", {length: 16, nullable: true})
    searchEmoteOutcomeType!: EmoteOutcomeType | undefined | null

    @StringColumn_({array: true, nullable: false})
    uniqueCollectors!: (string)[]

    @IntColumn_({nullable: false})
    uniqueCollectorsTotal!: number

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
