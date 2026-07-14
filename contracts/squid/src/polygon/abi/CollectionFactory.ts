import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    ImplementationChanged: event("0x39b832a8f081478030a2180aeddc64e58329325ac2e8d863a9ca2594a80c4103", "ImplementationChanged(address,bytes32,bytes)", {"_implementation": indexed(p.address), "_codeHash": p.bytes32, "_code": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    ProxyCreated: event("0xcfaab0d6675a72a93c114f48dd85add1076be0c88545968759ef034da7ad146f", "ProxyCreated(address,bytes32)", {"_address": indexed(p.address), "_salt": p.bytes32}),
}

export const functions = {
    code: viewFun("0x24c12bf6", "code()", {}, p.bytes),
    codeHash: viewFun("0x18edaaf2", "codeHash()", {}, p.bytes32),
    createCollection: fun("0x9d2d3957", "createCollection(bytes32,bytes)", {"_salt": p.bytes32, "_data": p.bytes}, p.address),
    createProxy: fun("0xd277dc89", "createProxy(bytes32,bytes)", {"_salt": p.bytes32, "_data": p.bytes}, p.address),
    getAddress: viewFun("0x20d06ff9", "getAddress(bytes32,address)", {"_salt": p.bytes32, "_address": p.address}, p.address),
    implementation: viewFun("0x5c60da1b", "implementation()", {}, p.address),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    setImplementation: fun("0xd784d426", "setImplementation(address)", {"_implementation": p.address}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    code() {
        return this.eth_call(functions.code, {})
    }

    codeHash() {
        return this.eth_call(functions.codeHash, {})
    }

    getAddress(_salt: GetAddressParams["_salt"], _address: GetAddressParams["_address"]) {
        return this.eth_call(functions.getAddress, {_salt, _address})
    }

    implementation() {
        return this.eth_call(functions.implementation, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }
}

/// Event types
export type ImplementationChangedEventArgs = EParams<typeof events.ImplementationChanged>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type ProxyCreatedEventArgs = EParams<typeof events.ProxyCreated>

/// Function types
export type CodeParams = FunctionArguments<typeof functions.code>
export type CodeReturn = FunctionReturn<typeof functions.code>

export type CodeHashParams = FunctionArguments<typeof functions.codeHash>
export type CodeHashReturn = FunctionReturn<typeof functions.codeHash>

export type CreateCollectionParams = FunctionArguments<typeof functions.createCollection>
export type CreateCollectionReturn = FunctionReturn<typeof functions.createCollection>

export type CreateProxyParams = FunctionArguments<typeof functions.createProxy>
export type CreateProxyReturn = FunctionReturn<typeof functions.createProxy>

export type GetAddressParams = FunctionArguments<typeof functions.getAddress>
export type GetAddressReturn = FunctionReturn<typeof functions.getAddress>

export type ImplementationParams = FunctionArguments<typeof functions.implementation>
export type ImplementationReturn = FunctionReturn<typeof functions.implementation>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type SetImplementationParams = FunctionArguments<typeof functions.setImplementation>
export type SetImplementationReturn = FunctionReturn<typeof functions.setImplementation>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

