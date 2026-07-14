import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    Bought: event("0x77cc75f8061aa168906862622e88c5b05a026a9c06c02d91ec98543e01e7ad33", "Bought((address,uint256[],uint256[],address[])[])", {"_itemsToBuy": p.array(p.struct({"collection": p.address, "ids": p.array(p.uint256), "prices": p.array(p.uint256), "beneficiaries": p.array(p.address)}))}),
    MetaTransactionExecuted: event("0x5845892132946850460bff5a0083f71031bc5bf9aadcd40f1de79423eac9b10b", "MetaTransactionExecuted(address,address,bytes)", {"userAddress": p.address, "relayerAddress": p.address, "functionSignature": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    SetFee: event("0x032dc6a2d839eb179729a55633fdf1c41a1fc4739394154117005db2b354b9b5", "SetFee(uint256,uint256)", {"_oldFee": p.uint256, "_newFee": p.uint256}),
    SetFeeOwner: event("0xe0bbf1a07376101b84e5aff236bc710878c9a975168510f821b4a735c0d35e51", "SetFeeOwner(address,address)", {"_oldFeeOwner": indexed(p.address), "_newFeeOwner": indexed(p.address)}),
}

export const functions = {
    BASE_FEE: viewFun("0x3d18651e", "BASE_FEE()", {}, p.uint256),
    acceptedToken: viewFun("0x451c3d80", "acceptedToken()", {}, p.address),
    buy: fun("0xa4fdc78a", "buy((address,uint256[],uint256[],address[])[])", {"_itemsToBuy": p.array(p.struct({"collection": p.address, "ids": p.array(p.uint256), "prices": p.array(p.uint256), "beneficiaries": p.array(p.address)}))}, ),
    domainSeparator: viewFun("0xf698da25", "domainSeparator()", {}, p.bytes32),
    executeMetaTransaction: fun("0x0c53c51c", "executeMetaTransaction(address,bytes,bytes32,bytes32,uint8)", {"userAddress": p.address, "functionSignature": p.bytes, "sigR": p.bytes32, "sigS": p.bytes32, "sigV": p.uint8}, p.bytes),
    fee: viewFun("0xddca3f43", "fee()", {}, p.uint256),
    feeOwner: viewFun("0xb9818be1", "feeOwner()", {}, p.address),
    getChainId: viewFun("0x3408e470", "getChainId()", {}, p.uint256),
    getItemBuyData: viewFun("0xe0f307c2", "getItemBuyData(address,uint256)", {"_collection": p.address, "_itemId": p.uint256}, {"_0": p.uint256, "_1": p.address}),
    getNonce: viewFun("0x2d0335ab", "getNonce(address)", {"user": p.address}, p.uint256),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    setFee: fun("0x69fe0e2d", "setFee(uint256)", {"_newFee": p.uint256}, ),
    setFeeOwner: fun("0x4b104eff", "setFeeOwner(address)", {"_newFeeOwner": p.address}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    BASE_FEE() {
        return this.eth_call(functions.BASE_FEE, {})
    }

    acceptedToken() {
        return this.eth_call(functions.acceptedToken, {})
    }

    domainSeparator() {
        return this.eth_call(functions.domainSeparator, {})
    }

    fee() {
        return this.eth_call(functions.fee, {})
    }

    feeOwner() {
        return this.eth_call(functions.feeOwner, {})
    }

    getChainId() {
        return this.eth_call(functions.getChainId, {})
    }

    getItemBuyData(_collection: GetItemBuyDataParams["_collection"], _itemId: GetItemBuyDataParams["_itemId"]) {
        return this.eth_call(functions.getItemBuyData, {_collection, _itemId})
    }

    getNonce(user: GetNonceParams["user"]) {
        return this.eth_call(functions.getNonce, {user})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }
}

/// Event types
export type BoughtEventArgs = EParams<typeof events.Bought>
export type MetaTransactionExecutedEventArgs = EParams<typeof events.MetaTransactionExecuted>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type SetFeeEventArgs = EParams<typeof events.SetFee>
export type SetFeeOwnerEventArgs = EParams<typeof events.SetFeeOwner>

/// Function types
export type BASE_FEEParams = FunctionArguments<typeof functions.BASE_FEE>
export type BASE_FEEReturn = FunctionReturn<typeof functions.BASE_FEE>

export type AcceptedTokenParams = FunctionArguments<typeof functions.acceptedToken>
export type AcceptedTokenReturn = FunctionReturn<typeof functions.acceptedToken>

export type BuyParams = FunctionArguments<typeof functions.buy>
export type BuyReturn = FunctionReturn<typeof functions.buy>

export type DomainSeparatorParams = FunctionArguments<typeof functions.domainSeparator>
export type DomainSeparatorReturn = FunctionReturn<typeof functions.domainSeparator>

export type ExecuteMetaTransactionParams = FunctionArguments<typeof functions.executeMetaTransaction>
export type ExecuteMetaTransactionReturn = FunctionReturn<typeof functions.executeMetaTransaction>

export type FeeParams = FunctionArguments<typeof functions.fee>
export type FeeReturn = FunctionReturn<typeof functions.fee>

export type FeeOwnerParams = FunctionArguments<typeof functions.feeOwner>
export type FeeOwnerReturn = FunctionReturn<typeof functions.feeOwner>

export type GetChainIdParams = FunctionArguments<typeof functions.getChainId>
export type GetChainIdReturn = FunctionReturn<typeof functions.getChainId>

export type GetItemBuyDataParams = FunctionArguments<typeof functions.getItemBuyData>
export type GetItemBuyDataReturn = FunctionReturn<typeof functions.getItemBuyData>

export type GetNonceParams = FunctionArguments<typeof functions.getNonce>
export type GetNonceReturn = FunctionReturn<typeof functions.getNonce>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type SetFeeParams = FunctionArguments<typeof functions.setFee>
export type SetFeeReturn = FunctionReturn<typeof functions.setFee>

export type SetFeeOwnerParams = FunctionArguments<typeof functions.setFeeOwner>
export type SetFeeOwnerReturn = FunctionReturn<typeof functions.setFeeOwner>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

