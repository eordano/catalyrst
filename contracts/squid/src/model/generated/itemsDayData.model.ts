import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_, StringColumn as StringColumn_} from "@subsquid/typeorm-store"
import {EmoteCategory} from "./_emoteCategory"
import {WearableCategory} from "./_wearableCategory"

@Entity_()
export class ItemsDayData {
    constructor(props?: Partial<ItemsDayData>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @IntColumn_({nullable: false})
    date!: number

    @IntColumn_({nullable: false})
    sales!: number

    @BigIntColumn_({nullable: false})
    volume!: bigint

    @Column_("varchar", {length: 13, nullable: true})
    searchEmoteCategory!: EmoteCategory | undefined | null

    @Column_("varchar", {length: 11, nullable: true})
    searchWearableCategory!: WearableCategory | undefined | null

    @StringColumn_({nullable: true})
    searchRarity!: string | undefined | null
}
