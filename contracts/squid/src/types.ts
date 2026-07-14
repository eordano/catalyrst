import { Network as DCLNetwork } from "@dcl/schemas";

export type Network = DCLNetwork.ETHEREUM | DCLNetwork.MATIC;

export type Coordinate = {
  readonly _0: bigint;
  readonly _1: bigint;
};
