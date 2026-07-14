import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, StringColumn as StringColumn_, BooleanColumn as BooleanColumn_} from "@subsquid/typeorm-store"
import {EmoteCategory} from "./_emoteCategory"
import {WearableRarity} from "./_wearableRarity"
import {WearableBodyShape} from "./_wearableBodyShape"
import {EmoteOutcomeType} from "./_emoteOutcomeType"

@Entity_()
export class Emote {
    constructor(props?: Partial<Emote>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @StringColumn_({nullable: false})
    name!: string

    @StringColumn_({nullable: false})
    description!: string

    @StringColumn_({nullable: false})
    collection!: string

    @Column_("varchar", {length: 13, nullable: false})
    category!: EmoteCategory

    @BooleanColumn_({nullable: false})
    loop!: boolean

    @Column_("varchar", {length: 9, nullable: false})
    rarity!: WearableRarity

    @Column_("varchar", {length: 10, array: true, nullable: true})
    bodyShapes!: (WearableBodyShape)[] | undefined | null

    @BooleanColumn_({nullable: true})
    hasSound!: boolean | undefined | null

    @BooleanColumn_({nullable: true})
    hasGeometry!: boolean | undefined | null

    @Column_("varchar", {length: 16, nullable: true})
    outcomeType!: EmoteOutcomeType | undefined | null
}
