import {
  Entity as Entity_,
  Column as Column_,
  PrimaryColumn as PrimaryColumn_,
  StringColumn as StringColumn_,
  Index as Index_,
  BigIntColumn as BigIntColumn_,
} from "@subsquid/typeorm-store";
import { Network } from "./_network";

@Entity_()
export class SquidRouterOrder {
  constructor(props?: Partial<SquidRouterOrder>) {
    Object.assign(this, props);
  }

  @PrimaryColumn_()
  id!: string;

  @Index_()
  @StringColumn_({ nullable: false })
  orderHash!: string;

  @StringColumn_({ array: true, nullable: false })
  creditIds!: string[];

  @BigIntColumn_({ nullable: false })
  totalCreditsUsed!: bigint;

  @Index_()
  @StringColumn_({ nullable: false })
  txHash!: string;

  @BigIntColumn_({ nullable: false })
  blockNumber!: bigint;

  @BigIntColumn_({ nullable: false })
  timestamp!: bigint;

  @Column_("varchar", { length: 8, nullable: false })
  network!: Network;
}
