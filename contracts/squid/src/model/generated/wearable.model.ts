import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, ManyToOne as ManyToOne_, Index as Index_, StringColumn as StringColumn_, OneToOne as OneToOne_} from "@subsquid/typeorm-store"
import {Account} from "./account.model"
import {WearableCategory} from "./_wearableCategory"
import {WearableRarity} from "./_wearableRarity"
import {WearableBodyShape} from "./_wearableBodyShape"
import {NFT} from "./nft.model"
import {Network} from "./_network"

@Entity_()
export class Wearable {
    constructor(props?: Partial<Wearable>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    owner!: Account | undefined | null

    @StringColumn_({nullable: true})
    representationId!: string | undefined | null

    @StringColumn_({nullable: false})
    collection!: string

    @StringColumn_({nullable: false})
    name!: string

    @StringColumn_({nullable: false})
    description!: string

    @Column_("varchar", {length: 11, nullable: false})
    category!: WearableCategory

    @Column_("varchar", {length: 9, nullable: false})
    rarity!: WearableRarity

    @Column_("varchar", {length: 10, array: true, nullable: true})
    bodyShapes!: (WearableBodyShape)[] | undefined | null

    @OneToOne_(() => NFT, e => e.wearable)
    nft!: NFT | undefined | null

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
