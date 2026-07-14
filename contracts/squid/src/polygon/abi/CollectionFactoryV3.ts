import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    ImplementationSet: event("0x7acba6e3c47815c1b41b230650f7ed720d66bdc6ca45468c4a6cdb9425132024", "ImplementationSet(address,bytes32,bytes)", {"_implementation": indexed(p.address), "_codeHash": p.bytes32, "_code": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    ProxyCreated: event("0xcfaab0d6675a72a93c114f48dd85add1076be0c88545968759ef034da7ad146f", "ProxyCreated(address,bytes32)", {"_address": indexed(p.address), "_salt": p.bytes32}),
}

export const functions = {
    code: viewFun("0x24c12bf6", "code()", {}, p.bytes),
    codeHash: viewFun("0x18edaaf2", "codeHash()", {}, p.bytes32),
    collections: viewFun("0xfdbda0ec", "collections(uint256)", {"_0": p.uint256}, p.address),
    collectionsSize: viewFun("0xdfaf33e9", "collectionsSize()", {}, p.uint256),
    createCollection: fun("0x9d2d3957", "createCollection(bytes32,bytes)", {"_salt": p.bytes32, "_data": p.bytes}, p.address),
    getAddress: viewFun("0xaf222399", "getAddress(bytes32,address,bytes)", {"_salt": p.bytes32, "_address": p.address, "_data": p.bytes}, p.address),
    implementation: viewFun("0x5c60da1b", "implementation()", {}, p.address),
    isCollectionFromFactory: viewFun("0xc0300011", "isCollectionFromFactory(address)", {"_0": p.address}, p.bool),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    code() {
        return this.eth_call(functions.code, {})
    }

    codeHash() {
        return this.eth_call(functions.codeHash, {})
    }

    collections(_0: CollectionsParams["_0"]) {
        return this.eth_call(functions.collections, {_0})
    }

    collectionsSize() {
        return this.eth_call(functions.collectionsSize, {})
    }

    getAddress(_salt: GetAddressParams["_salt"], _address: GetAddressParams["_address"], _data: GetAddressParams["_data"]) {
        return this.eth_call(functions.getAddress, {_salt, _address, _data})
    }

    implementation() {
        return this.eth_call(functions.implementation, {})
    }

    isCollectionFromFactory(_0: IsCollectionFromFactoryParams["_0"]) {
        return this.eth_call(functions.isCollectionFromFactory, {_0})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }
}

/// Event types
export type ImplementationSetEventArgs = EParams<typeof events.ImplementationSet>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type ProxyCreatedEventArgs = EParams<typeof events.ProxyCreated>

/// Function types
export type CodeParams = FunctionArguments<typeof functions.code>
export type CodeReturn = FunctionReturn<typeof functions.code>

export type CodeHashParams = FunctionArguments<typeof functions.codeHash>
export type CodeHashReturn = FunctionReturn<typeof functions.codeHash>

export type CollectionsParams = FunctionArguments<typeof functions.collections>
export type CollectionsReturn = FunctionReturn<typeof functions.collections>

export type CollectionsSizeParams = FunctionArguments<typeof functions.collectionsSize>
export type CollectionsSizeReturn = FunctionReturn<typeof functions.collectionsSize>

export type CreateCollectionParams = FunctionArguments<typeof functions.createCollection>
export type CreateCollectionReturn = FunctionReturn<typeof functions.createCollection>

export type GetAddressParams = FunctionArguments<typeof functions.getAddress>
export type GetAddressReturn = FunctionReturn<typeof functions.getAddress>

export type ImplementationParams = FunctionArguments<typeof functions.implementation>
export type ImplementationReturn = FunctionReturn<typeof functions.implementation>

export type IsCollectionFromFactoryParams = FunctionArguments<typeof functions.isCollectionFromFactory>
export type IsCollectionFromFactoryReturn = FunctionReturn<typeof functions.isCollectionFromFactory>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

