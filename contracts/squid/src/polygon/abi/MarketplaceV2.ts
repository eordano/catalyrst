import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    ChangedFeesCollectorCutPerMillion: event("0xd33ed052f0041ea5512df480eb51801acb8fec4a7e89c6086fdd5caaa650981e", "ChangedFeesCollectorCutPerMillion(uint256)", {"feesCollectorCutPerMillion": p.uint256}),
    ChangedPublicationFee: event("0xe7fa8737293f41b5dfa0d5c3e552860a06275bed7015581b083c7be7003308ba", "ChangedPublicationFee(uint256)", {"publicationFee": p.uint256}),
    ChangedRoyaltiesCutPerMillion: event("0xba54fecf3e91476de2d7bd3248ee80a9060cddc82a6ac0933b232e051eafdaf3", "ChangedRoyaltiesCutPerMillion(uint256)", {"royaltiesCutPerMillion": p.uint256}),
    FeesCollectorSet: event("0x58283f7e46512bbcb30558fa508283f23fac7be18fb20bfdf6d4bbf83c529d53", "FeesCollectorSet(address,address)", {"oldFeesCollector": indexed(p.address), "newFeesCollector": indexed(p.address)}),
    MetaTransactionExecuted: event("0x5845892132946850460bff5a0083f71031bc5bf9aadcd40f1de79423eac9b10b", "MetaTransactionExecuted(address,address,bytes)", {"userAddress": p.address, "relayerAddress": p.address, "functionSignature": p.bytes}),
    OrderCancelled: event("0x0325426328de5b91ae4ad8462ad4076de4bcaf4551e81556185cacde5a425c6b", "OrderCancelled(bytes32,uint256,address,address)", {"id": p.bytes32, "assetId": indexed(p.uint256), "seller": indexed(p.address), "nftAddress": p.address}),
    OrderCreated: event("0x84c66c3f7ba4b390e20e8e8233e2a516f3ce34a72749e4f12bd010dfba238039", "OrderCreated(bytes32,uint256,address,address,uint256,uint256)", {"id": p.bytes32, "assetId": indexed(p.uint256), "seller": indexed(p.address), "nftAddress": p.address, "priceInWei": p.uint256, "expiresAt": p.uint256}),
    OrderSuccessful: event("0x695ec315e8a642a74d450a4505eeea53df699b47a7378c7d752e97d5b16eb9bb", "OrderSuccessful(bytes32,uint256,address,address,uint256,address)", {"id": p.bytes32, "assetId": indexed(p.uint256), "seller": indexed(p.address), "nftAddress": p.address, "totalPrice": p.uint256, "buyer": indexed(p.address)}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    Paused: event("0x62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258", "Paused(address)", {"account": p.address}),
    RoyaltiesManagerSet: event("0x7af757e60ce7c97cfab72d633ae4d05ffe9be9afbba020aa2f484a600db8df6d", "RoyaltiesManagerSet(address,address)", {"oldRoyaltiesManager": indexed(p.address), "newRoyaltiesManager": indexed(p.address)}),
    Unpaused: event("0x5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa", "Unpaused(address)", {"account": p.address}),
}

export const functions = {
    ERC721_Interface: viewFun("0x2b4c32be", "ERC721_Interface()", {}, p.bytes4),
    InterfaceId_ValidateFingerprint: viewFun("0x37f82f37", "InterfaceId_ValidateFingerprint()", {}, p.bytes4),
    acceptedToken: viewFun("0x451c3d80", "acceptedToken()", {}, p.address),
    cancelOrder: fun("0x6a206137", "cancelOrder(address,uint256)", {"nftAddress": p.address, "assetId": p.uint256}, ),
    createOrder: fun("0x6f652e1a", "createOrder(address,uint256,uint256,uint256)", {"nftAddress": p.address, "assetId": p.uint256, "priceInWei": p.uint256, "expiresAt": p.uint256}, ),
    domainSeparator: viewFun("0xf698da25", "domainSeparator()", {}, p.bytes32),
    executeMetaTransaction: fun("0x0c53c51c", "executeMetaTransaction(address,bytes,bytes32,bytes32,uint8)", {"userAddress": p.address, "functionSignature": p.bytes, "sigR": p.bytes32, "sigS": p.bytes32, "sigV": p.uint8}, p.bytes),
    executeOrder: fun("0xae7b0333", "executeOrder(address,uint256,uint256)", {"nftAddress": p.address, "assetId": p.uint256, "price": p.uint256}, ),
    feesCollector: viewFun("0x9cf160f6", "feesCollector()", {}, p.address),
    feesCollectorCutPerMillion: viewFun("0x48f4e32b", "feesCollectorCutPerMillion()", {}, p.uint256),
    getChainId: viewFun("0x3408e470", "getChainId()", {}, p.uint256),
    getNonce: viewFun("0x2d0335ab", "getNonce(address)", {"user": p.address}, p.uint256),
    orderByAssetId: viewFun("0xe61f3851", "orderByAssetId(address,uint256)", {"_0": p.address, "_1": p.uint256}, {"id": p.bytes32, "seller": p.address, "nftAddress": p.address, "price": p.uint256, "expiresAt": p.uint256}),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    paused: viewFun("0x5c975abb", "paused()", {}, p.bool),
    publicationFeeInWei: viewFun("0xae4f1198", "publicationFeeInWei()", {}, p.uint256),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    royaltiesCutPerMillion: viewFun("0xce2243b4", "royaltiesCutPerMillion()", {}, p.uint256),
    royaltiesManager: viewFun("0x967b0049", "royaltiesManager()", {}, p.address),
    safeExecuteOrder: fun("0x9b214f77", "safeExecuteOrder(address,uint256,uint256,bytes)", {"nftAddress": p.address, "assetId": p.uint256, "price": p.uint256, "fingerprint": p.bytes}, ),
    setFeesCollector: fun("0x373071f2", "setFeesCollector(address)", {"_newFeesCollector": p.address}, ),
    setFeesCollectorCutPerMillion: fun("0x8780dfcf", "setFeesCollectorCutPerMillion(uint256)", {"_feesCollectorCutPerMillion": p.uint256}, ),
    setPublicationFee: fun("0xaf8996f1", "setPublicationFee(uint256)", {"_publicationFee": p.uint256}, ),
    setRoyaltiesCutPerMillion: fun("0x93dc06e1", "setRoyaltiesCutPerMillion(uint256)", {"_royaltiesCutPerMillion": p.uint256}, ),
    setRoyaltiesManager: fun("0x688f09da", "setRoyaltiesManager(address)", {"_newRoyaltiesManager": p.address}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    ERC721_Interface() {
        return this.eth_call(functions.ERC721_Interface, {})
    }

    InterfaceId_ValidateFingerprint() {
        return this.eth_call(functions.InterfaceId_ValidateFingerprint, {})
    }

    acceptedToken() {
        return this.eth_call(functions.acceptedToken, {})
    }

    domainSeparator() {
        return this.eth_call(functions.domainSeparator, {})
    }

    feesCollector() {
        return this.eth_call(functions.feesCollector, {})
    }

    feesCollectorCutPerMillion() {
        return this.eth_call(functions.feesCollectorCutPerMillion, {})
    }

    getChainId() {
        return this.eth_call(functions.getChainId, {})
    }

    getNonce(user: GetNonceParams["user"]) {
        return this.eth_call(functions.getNonce, {user})
    }

    orderByAssetId(_0: OrderByAssetIdParams["_0"], _1: OrderByAssetIdParams["_1"]) {
        return this.eth_call(functions.orderByAssetId, {_0, _1})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    paused() {
        return this.eth_call(functions.paused, {})
    }

    publicationFeeInWei() {
        return this.eth_call(functions.publicationFeeInWei, {})
    }

    royaltiesCutPerMillion() {
        return this.eth_call(functions.royaltiesCutPerMillion, {})
    }

    royaltiesManager() {
        return this.eth_call(functions.royaltiesManager, {})
    }
}

/// Event types
export type ChangedFeesCollectorCutPerMillionEventArgs = EParams<typeof events.ChangedFeesCollectorCutPerMillion>
export type ChangedPublicationFeeEventArgs = EParams<typeof events.ChangedPublicationFee>
export type ChangedRoyaltiesCutPerMillionEventArgs = EParams<typeof events.ChangedRoyaltiesCutPerMillion>
export type FeesCollectorSetEventArgs = EParams<typeof events.FeesCollectorSet>
export type MetaTransactionExecutedEventArgs = EParams<typeof events.MetaTransactionExecuted>
export type OrderCancelledEventArgs = EParams<typeof events.OrderCancelled>
export type OrderCreatedEventArgs = EParams<typeof events.OrderCreated>
export type OrderSuccessfulEventArgs = EParams<typeof events.OrderSuccessful>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type PausedEventArgs = EParams<typeof events.Paused>
export type RoyaltiesManagerSetEventArgs = EParams<typeof events.RoyaltiesManagerSet>
export type UnpausedEventArgs = EParams<typeof events.Unpaused>

/// Function types
export type ERC721_InterfaceParams = FunctionArguments<typeof functions.ERC721_Interface>
export type ERC721_InterfaceReturn = FunctionReturn<typeof functions.ERC721_Interface>

export type InterfaceId_ValidateFingerprintParams = FunctionArguments<typeof functions.InterfaceId_ValidateFingerprint>
export type InterfaceId_ValidateFingerprintReturn = FunctionReturn<typeof functions.InterfaceId_ValidateFingerprint>

export type AcceptedTokenParams = FunctionArguments<typeof functions.acceptedToken>
export type AcceptedTokenReturn = FunctionReturn<typeof functions.acceptedToken>

export type CancelOrderParams = FunctionArguments<typeof functions.cancelOrder>
export type CancelOrderReturn = FunctionReturn<typeof functions.cancelOrder>

export type CreateOrderParams = FunctionArguments<typeof functions.createOrder>
export type CreateOrderReturn = FunctionReturn<typeof functions.createOrder>

export type DomainSeparatorParams = FunctionArguments<typeof functions.domainSeparator>
export type DomainSeparatorReturn = FunctionReturn<typeof functions.domainSeparator>

export type ExecuteMetaTransactionParams = FunctionArguments<typeof functions.executeMetaTransaction>
export type ExecuteMetaTransactionReturn = FunctionReturn<typeof functions.executeMetaTransaction>

export type ExecuteOrderParams = FunctionArguments<typeof functions.executeOrder>
export type ExecuteOrderReturn = FunctionReturn<typeof functions.executeOrder>

export type FeesCollectorParams = FunctionArguments<typeof functions.feesCollector>
export type FeesCollectorReturn = FunctionReturn<typeof functions.feesCollector>

export type FeesCollectorCutPerMillionParams = FunctionArguments<typeof functions.feesCollectorCutPerMillion>
export type FeesCollectorCutPerMillionReturn = FunctionReturn<typeof functions.feesCollectorCutPerMillion>

export type GetChainIdParams = FunctionArguments<typeof functions.getChainId>
export type GetChainIdReturn = FunctionReturn<typeof functions.getChainId>

export type GetNonceParams = FunctionArguments<typeof functions.getNonce>
export type GetNonceReturn = FunctionReturn<typeof functions.getNonce>

export type OrderByAssetIdParams = FunctionArguments<typeof functions.orderByAssetId>
export type OrderByAssetIdReturn = FunctionReturn<typeof functions.orderByAssetId>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type PausedParams = FunctionArguments<typeof functions.paused>
export type PausedReturn = FunctionReturn<typeof functions.paused>

export type PublicationFeeInWeiParams = FunctionArguments<typeof functions.publicationFeeInWei>
export type PublicationFeeInWeiReturn = FunctionReturn<typeof functions.publicationFeeInWei>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type RoyaltiesCutPerMillionParams = FunctionArguments<typeof functions.royaltiesCutPerMillion>
export type RoyaltiesCutPerMillionReturn = FunctionReturn<typeof functions.royaltiesCutPerMillion>

export type RoyaltiesManagerParams = FunctionArguments<typeof functions.royaltiesManager>
export type RoyaltiesManagerReturn = FunctionReturn<typeof functions.royaltiesManager>

export type SafeExecuteOrderParams = FunctionArguments<typeof functions.safeExecuteOrder>
export type SafeExecuteOrderReturn = FunctionReturn<typeof functions.safeExecuteOrder>

export type SetFeesCollectorParams = FunctionArguments<typeof functions.setFeesCollector>
export type SetFeesCollectorReturn = FunctionReturn<typeof functions.setFeesCollector>

export type SetFeesCollectorCutPerMillionParams = FunctionArguments<typeof functions.setFeesCollectorCutPerMillion>
export type SetFeesCollectorCutPerMillionReturn = FunctionReturn<typeof functions.setFeesCollectorCutPerMillion>

export type SetPublicationFeeParams = FunctionArguments<typeof functions.setPublicationFee>
export type SetPublicationFeeReturn = FunctionReturn<typeof functions.setPublicationFee>

export type SetRoyaltiesCutPerMillionParams = FunctionArguments<typeof functions.setRoyaltiesCutPerMillion>
export type SetRoyaltiesCutPerMillionReturn = FunctionReturn<typeof functions.setRoyaltiesCutPerMillion>

export type SetRoyaltiesManagerParams = FunctionArguments<typeof functions.setRoyaltiesManager>
export type SetRoyaltiesManagerReturn = FunctionReturn<typeof functions.setRoyaltiesManager>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

