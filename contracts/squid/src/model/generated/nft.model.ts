import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, Index as Index_, BigIntColumn as BigIntColumn_, StringColumn as StringColumn_, ManyToOne as ManyToOne_, OneToMany as OneToMany_, OneToOne as OneToOne_, JoinColumn as JoinColumn_, IntColumn as IntColumn_, DateTimeColumn as DateTimeColumn_, BooleanColumn as BooleanColumn_} from "@subsquid/typeorm-store"
import {Category} from "./_category"
import {Account} from "./account.model"
import {Order} from "./order.model"
import {Bid} from "./bid.model"
import {Parcel} from "./parcel.model"
import {Estate} from "./estate.model"
import {Wearable} from "./wearable.model"
import {ENS} from "./ens.model"
import {OrderStatus} from "./_orderStatus"
import {WearableCategory} from "./_wearableCategory"
import {WearableBodyShape} from "./_wearableBodyShape"
import {ItemType} from "./_itemType"
import {Collection} from "./collection.model"
import {Item} from "./item.model"
import {Metadata} from "./metadata.model"
import {EmoteCategory} from "./_emoteCategory"
import {Network} from "./_network"

@Index_(["item", "owner"], {unique: false})
@Index_(["contractAddress", "tokenId"], {unique: false})
@Index_(["searchOrderStatus", "searchOrderExpiresAt", "network"], {unique: false})
@Index_(["searchOrderStatus", "searchOrderExpiresAt", "category"], {unique: false})
@Entity_()
export class NFT {
    constructor(props?: Partial<NFT>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Index_()
    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @StringColumn_({nullable: false})
    contractAddress!: string

    @Index_()
    @Column_("varchar", {length: 8, nullable: false})
    category!: Category

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    owner!: Account

    @Index_()
    @StringColumn_({nullable: false})
    ownerAddress!: string

    @StringColumn_({nullable: true})
    tokenURI!: string | undefined | null

    @OneToMany_(() => Order, e => e.nft)
    orders!: Order[]

    @OneToMany_(() => Bid, e => e.nft)
    bids!: Bid[]

    @Index_()
    @ManyToOne_(() => Order, {nullable: true})
    activeOrder!: Order | undefined | null

    @StringColumn_({nullable: true})
    name!: string | undefined | null

    @StringColumn_({nullable: true})
    image!: string | undefined | null

    @Index_({unique: true})
    @OneToOne_(() => Parcel, {nullable: true})
    @JoinColumn_()
    parcel!: Parcel | undefined | null

    @Index_({unique: true})
    @OneToOne_(() => Estate, {nullable: true})
    @JoinColumn_()
    estate!: Estate | undefined | null

    @Index_({unique: true})
    @OneToOne_(() => Wearable, {nullable: true})
    @JoinColumn_()
    wearable!: Wearable | undefined | null

    @Index_({unique: true})
    @OneToOne_(() => ENS, {nullable: true})
    @JoinColumn_()
    ens!: ENS | undefined | null

    @Index_()
    @BigIntColumn_({nullable: false})
    createdAt!: bigint

    @BigIntColumn_({nullable: false})
    updatedAt!: bigint

    @BigIntColumn_({nullable: true})
    soldAt!: bigint | undefined | null

    @BigIntColumn_({nullable: false})
    transferredAt!: bigint

    @IntColumn_({nullable: false})
    sales!: number

    @BigIntColumn_({nullable: false})
    volume!: bigint

    @Column_("varchar", {length: 11, nullable: true})
    searchOrderStatus!: OrderStatus | undefined | null

    @BigIntColumn_({nullable: true})
    searchOrderPrice!: bigint | undefined | null

    @Index_()
    @BigIntColumn_({nullable: true})
    searchOrderExpiresAt!: bigint | undefined | null

    @Index_()
    @DateTimeColumn_({nullable: true})
    searchOrderExpiresAtNormalized!: Date | undefined | null

    @BigIntColumn_({nullable: true})
    searchOrderCreatedAt!: bigint | undefined | null

    @Index_()
    @BooleanColumn_({nullable: true})
    searchIsLand!: boolean | undefined | null

    @StringColumn_({nullable: true})
    searchText!: string | undefined | null

    @Index_()
    @BooleanColumn_({nullable: true})
    searchParcelIsInBounds!: boolean | undefined | null

    @BigIntColumn_({nullable: true})
    searchParcelX!: bigint | undefined | null

    @BigIntColumn_({nullable: true})
    searchParcelY!: bigint | undefined | null

    @StringColumn_({nullable: true})
    searchParcelEstateId!: string | undefined | null

    @IntColumn_({nullable: true})
    searchDistanceToPlaza!: number | undefined | null

    @BooleanColumn_({nullable: true})
    searchAdjacentToRoad!: boolean | undefined | null

    @IntColumn_({nullable: true})
    searchEstateSize!: number | undefined | null

    @BooleanColumn_({nullable: true})
    searchIsWearableHead!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    searchIsWearableAccessory!: boolean | undefined | null

    @StringColumn_({nullable: true})
    searchWearableRarity!: string | undefined | null

    @Column_("varchar", {length: 11, nullable: true})
    searchWearableCategory!: WearableCategory | undefined | null

    @Column_("varchar", {length: 10, array: true, nullable: true})
    searchWearableBodyShapes!: (WearableBodyShape)[] | undefined | null

    @BigIntColumn_({nullable: true})
    itemBlockchainId!: bigint | undefined | null

    @BigIntColumn_({nullable: true})
    issuedId!: bigint | undefined | null

    @Column_("varchar", {length: 17, nullable: true})
    itemType!: ItemType | undefined | null

    @StringColumn_({nullable: true})
    urn!: string | undefined | null

    @Index_()
    @ManyToOne_(() => Collection, {nullable: true})
    collection!: Collection | undefined | null

    @ManyToOne_(() => Item, {nullable: true})
    item!: Item | undefined | null

    @Index_()
    @ManyToOne_(() => Metadata, {nullable: true})
    metadata!: Metadata | undefined | null

    @StringColumn_({nullable: true})
    searchItemType!: string | undefined | null

    @Column_("varchar", {length: 13, nullable: true})
    searchEmoteCategory!: EmoteCategory | undefined | null

    @BooleanColumn_({nullable: true})
    searchEmoteLoop!: boolean | undefined | null

    @StringColumn_({nullable: true})
    searchEmoteRarity!: string | undefined | null

    @Column_("varchar", {length: 10, array: true, nullable: true})
    searchEmoteBodyShapes!: (WearableBodyShape | undefined | null)[] | undefined | null

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
