import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, IntColumn as IntColumn_, BigIntColumn as BigIntColumn_} from "@subsquid/typeorm-store"
import {Network} from "./_network"

@Entity_()
export class AnalyticsDayData {
    constructor(props?: Partial<AnalyticsDayData>) {
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

    @BigIntColumn_({nullable: false})
    creatorsEarnings!: bigint

    @BigIntColumn_({nullable: false})
    daoEarnings!: bigint

    @Column_("varchar", {length: 8, nullable: false})
    network!: Network
}
