import * as p from "@subsquid/evm-codec";
import { event, fun, viewFun, indexed, ContractBase } from "@subsquid/evm-abi";
import type {
  EventParams as EParams,
  FunctionArguments,
  FunctionReturn,
} from "@subsquid/evm-abi";

export const events = {
  CreditUsed: event(
    "0xa68d6ae15d7c3b8ca4d13dbccb4762b825753f5bbafab66b88d893af0c1b7179",
    "CreditUsed(address,bytes32,(uint256,uint256,bytes32),uint256)",
    {
      _sender: indexed(p.address),
      _creditId: indexed(p.bytes32),
      _credit: p.struct({
        value: p.uint256,
        expiresAt: p.uint256,
        salt: p.bytes32,
      }),
      _value: p.uint256,
    }
  ),
  CreditsUsed: event(
    "0xbf0c0494baf6ed7e1481b0ec6d3ed75f70442f3ac8d509e54e80251640471373",
    "CreditsUsed(address,uint256,uint256)",
    {
      _sender: indexed(p.address),
      _manaTransferred: p.uint256,
      _creditedValue: p.uint256,
    }
  ),
};

export const functions = {};

export class Contract extends ContractBase {}

/// Event types
export type CreditUsedEventArgs = EParams<typeof events.CreditUsed>;
export type CreditsUsedEventArgs = EParams<typeof events.CreditsUsed>;

/// Function types
