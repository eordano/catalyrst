import {Entity as Entity_, Column as Column_, PrimaryColumn as PrimaryColumn_, BigIntColumn as BigIntColumn_, ManyToOne as ManyToOne_, Index as Index_, StringColumn as StringColumn_, OneToOne as OneToOne_} from "@subsquid/typeorm-store"
import {Account} from "./account.model"
import {NFT} from "./nft.model"

@Entity_()
export class ENS {
    constructor(props?: Partial<ENS>) {
        Object.assign(this, props)
    }

    @PrimaryColumn_()
    id!: string

    @BigIntColumn_({nullable: false})
    tokenId!: bigint

    @Index_()
    @ManyToOne_(() => Account, {nullable: true})
    owner!: Account

    @StringColumn_({nullable: true})
    caller!: string | undefined | null

    @StringColumn_({nullable: true})
    beneficiary!: string | undefined | null

    @StringColumn_({nullable: true})
    labelHash!: string | undefined | null

    @StringColumn_({nullable: true})
    subdomain!: string | undefined | null

    @BigIntColumn_({nullable: true})
    createdAt!: bigint | undefined | null

    @Index_()
    @StringColumn_({nullable: true})
    orderHash!: string | undefined | null

    @OneToOne_(() => NFT, e => e.ens)
    nft!: NFT | undefined | null
}
