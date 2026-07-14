import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, ManyToOne as ManyToOne_, Index as Index_} from "@subsquid/typeorm-store"
import {ItemType} from "./_itemType"
import {Wearable} from "./wearable.model"
import {Emote} from "./emote.model"
import {Network} from "./_network"

@Entity_()
export class Metadata {
    constructor(props?: Partial<Metadata>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Column_("varchar", {length: 17, nullable: false})
    itemType!: ItemType

    @Index_()
    @ManyToOne_(() => Wearable, {nullable: true})
    wearable!: Wearable | undefined | null

    @Index_()
    @ManyToOne_(() => Emote, {nullable: true})
    emote!: Emote | undefined | null

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
