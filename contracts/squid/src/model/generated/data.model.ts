import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, ManyToOne as ManyToOne_, Index as Index_, StringColumn as StringColumn_} from "@subsquid/typeorm-store"
import {Parcel} from "./parcel.model"
import {Estate} from "./estate.model"

@Entity_()
export class Data {
    constructor(props?: Partial<Data>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @Index_()
    @ManyToOne_(() => Parcel, {nullable: true})
    parcel!: Parcel | undefined | null

    @Index_()
    @ManyToOne_(() => Estate, {nullable: true})
    estate!: Estate | undefined | null

    @StringColumn_({nullable: false})
    version!: string

    @StringColumn_({nullable: true})
    name!: string | undefined | null

    @StringColumn_({nullable: true})
    description!: string | undefined | null

    @StringColumn_({nullable: true})
    ipns!: string | undefined | null
}
