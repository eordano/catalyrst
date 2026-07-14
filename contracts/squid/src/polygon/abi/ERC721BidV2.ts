import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    BidAccepted: event("0x4e5ca6c6c06fa8d62f2930830e0d370de70f108bd89213de0b51141775e695bd", "BidAccepted(bytes32,address,uint256,address,address,uint256,uint256)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": p.address, "_seller": indexed(p.address), "_price": p.uint256, "_fee": p.uint256}),
    BidCancelled: event("0xc43098075c34b8b92567bd49f08f55e397ebf82dd82072e3eb1be525c4506f5f", "BidCancelled(bytes32,address,uint256,address)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": indexed(p.address)}),
    BidCreated: event("0xe45b7709f1eed66d6e28f43b32f2003da0f0c40b92f75a8994370516e048620f", "BidCreated(bytes32,address,uint256,address,uint256,uint256,bytes)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": indexed(p.address), "_price": p.uint256, "_expiresAt": p.uint256, "_fingerprint": p.bytes}),
    ChangedFeesCollectorCutPerMillion: event("0xd33ed052f0041ea5512df480eb51801acb8fec4a7e89c6086fdd5caaa650981e", "ChangedFeesCollectorCutPerMillion(uint256)", {"_feesCollectorCutPerMillion": p.uint256}),
    ChangedRoyaltiesCutPerMillion: event("0xba54fecf3e91476de2d7bd3248ee80a9060cddc82a6ac0933b232e051eafdaf3", "ChangedRoyaltiesCutPerMillion(uint256)", {"_royaltiesCutPerMillion": p.uint256}),
    FeesCollectorSet: event("0x58283f7e46512bbcb30558fa508283f23fac7be18fb20bfdf6d4bbf83c529d53", "FeesCollectorSet(address,address)", {"_oldFeesCollector": indexed(p.address), "_newFeesCollector": indexed(p.address)}),
    MetaTransactionExecuted: event("0x5845892132946850460bff5a0083f71031bc5bf9aadcd40f1de79423eac9b10b", "MetaTransactionExecuted(address,address,bytes)", {"userAddress": p.address, "relayerAddress": p.address, "functionSignature": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    Paused: event("0x62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258", "Paused(address)", {"account": p.address}),
    RoyaltiesManagerSet: event("0x7af757e60ce7c97cfab72d633ae4d05ffe9be9afbba020aa2f484a600db8df6d", "RoyaltiesManagerSet(address,address)", {"_oldRoyaltiesManager": indexed(p.address), "_newRoyaltiesManager": indexed(p.address)}),
    Unpaused: event("0x5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa", "Unpaused(address)", {"account": p.address}),
}

export const functions = {
    ERC721Composable_ValidateFingerprint: viewFun("0x12712f7c", "ERC721Composable_ValidateFingerprint()", {}, p.bytes4),
    ERC721_Interface: viewFun("0x2b4c32be", "ERC721_Interface()", {}, p.bytes4),
    ERC721_Received: viewFun("0x4c81a727", "ERC721_Received()", {}, p.bytes4),
    MAX_BID_DURATION: viewFun("0xa750066e", "MAX_BID_DURATION()", {}, p.uint256),
    MIN_BID_DURATION: viewFun("0xf88a1217", "MIN_BID_DURATION()", {}, p.uint256),
    ONE_MILLION: viewFun("0x9869b736", "ONE_MILLION()", {}, p.uint256),
    bidCounterByToken: viewFun("0x28bd196a", "bidCounterByToken(address,uint256)", {"_0": p.address, "_1": p.uint256}, p.uint256),
    bidIdByTokenAndBidder: viewFun("0x279dc1e4", "bidIdByTokenAndBidder(address,uint256,address)", {"_0": p.address, "_1": p.uint256, "_2": p.address}, p.bytes32),
    bidIndexByBidId: viewFun("0xb5eea644", "bidIndexByBidId(bytes32)", {"_0": p.bytes32}, p.uint256),
    cancelBid: fun("0x39b6b1e5", "cancelBid(address,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256}, ),
    domainSeparator: viewFun("0xf698da25", "domainSeparator()", {}, p.bytes32),
    executeMetaTransaction: fun("0x0c53c51c", "executeMetaTransaction(address,bytes,bytes32,bytes32,uint8)", {"userAddress": p.address, "functionSignature": p.bytes, "sigR": p.bytes32, "sigS": p.bytes32, "sigV": p.uint8}, p.bytes),
    feesCollector: viewFun("0x9cf160f6", "feesCollector()", {}, p.address),
    feesCollectorCutPerMillion: viewFun("0x48f4e32b", "feesCollectorCutPerMillion()", {}, p.uint256),
    getBidByBidder: viewFun("0x00f052f6", "getBidByBidder(address,uint256,address)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_bidder": p.address}, {"bidIndex": p.uint256, "bidId": p.bytes32, "bidder": p.address, "price": p.uint256, "expiresAt": p.uint256}),
    getBidByToken: viewFun("0x42a6c4dd", "getBidByToken(address,uint256,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_index": p.uint256}, {"_0": p.bytes32, "_1": p.address, "_2": p.uint256, "_3": p.uint256}),
    getChainId: viewFun("0x3408e470", "getChainId()", {}, p.uint256),
    getNonce: viewFun("0x2d0335ab", "getNonce(address)", {"user": p.address}, p.uint256),
    manaToken: viewFun("0x74c97c99", "manaToken()", {}, p.address),
    onERC721Received: fun("0x150b7a02", "onERC721Received(address,address,uint256,bytes)", {"_from": p.address, "_1": p.address, "_tokenId": p.uint256, "_data": p.bytes}, p.bytes4),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    pause: fun("0x8456cb59", "pause()", {}, ),
    paused: viewFun("0x5c975abb", "paused()", {}, p.bool),
    'placeBid(address,uint256,uint256,uint256)': fun("0x81281be8", "placeBid(address,uint256,uint256,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_price": p.uint256, "_duration": p.uint256}, ),
    'placeBid(address,uint256,uint256,uint256,bytes)': fun("0xac063725", "placeBid(address,uint256,uint256,uint256,bytes)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_price": p.uint256, "_duration": p.uint256, "_fingerprint": p.bytes}, ),
    removeExpiredBids: fun("0xce1056cb", "removeExpiredBids(address[],uint256[],address[])", {"_tokenAddresses": p.array(p.address), "_tokenIds": p.array(p.uint256), "_bidders": p.array(p.address)}, ),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    royaltiesCutPerMillion: viewFun("0xce2243b4", "royaltiesCutPerMillion()", {}, p.uint256),
    royaltiesManager: viewFun("0x967b0049", "royaltiesManager()", {}, p.address),
    setFeesCollector: fun("0x373071f2", "setFeesCollector(address)", {"_newFeesCollector": p.address}, ),
    setFeesCollectorCutPerMillion: fun("0x8780dfcf", "setFeesCollectorCutPerMillion(uint256)", {"_feesCollectorCutPerMillion": p.uint256}, ),
    setRoyaltiesCutPerMillion: fun("0x93dc06e1", "setRoyaltiesCutPerMillion(uint256)", {"_royaltiesCutPerMillion": p.uint256}, ),
    setRoyaltiesManager: fun("0x688f09da", "setRoyaltiesManager(address)", {"_newRoyaltiesManager": p.address}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    ERC721Composable_ValidateFingerprint() {
        return this.eth_call(functions.ERC721Composable_ValidateFingerprint, {})
    }

    ERC721_Interface() {
        return this.eth_call(functions.ERC721_Interface, {})
    }

    ERC721_Received() {
        return this.eth_call(functions.ERC721_Received, {})
    }

    MAX_BID_DURATION() {
        return this.eth_call(functions.MAX_BID_DURATION, {})
    }

    MIN_BID_DURATION() {
        return this.eth_call(functions.MIN_BID_DURATION, {})
    }

    ONE_MILLION() {
        return this.eth_call(functions.ONE_MILLION, {})
    }

    bidCounterByToken(_0: BidCounterByTokenParams["_0"], _1: BidCounterByTokenParams["_1"]) {
        return this.eth_call(functions.bidCounterByToken, {_0, _1})
    }

    bidIdByTokenAndBidder(_0: BidIdByTokenAndBidderParams["_0"], _1: BidIdByTokenAndBidderParams["_1"], _2: BidIdByTokenAndBidderParams["_2"]) {
        return this.eth_call(functions.bidIdByTokenAndBidder, {_0, _1, _2})
    }

    bidIndexByBidId(_0: BidIndexByBidIdParams["_0"]) {
        return this.eth_call(functions.bidIndexByBidId, {_0})
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

    getBidByBidder(_tokenAddress: GetBidByBidderParams["_tokenAddress"], _tokenId: GetBidByBidderParams["_tokenId"], _bidder: GetBidByBidderParams["_bidder"]) {
        return this.eth_call(functions.getBidByBidder, {_tokenAddress, _tokenId, _bidder})
    }

    getBidByToken(_tokenAddress: GetBidByTokenParams["_tokenAddress"], _tokenId: GetBidByTokenParams["_tokenId"], _index: GetBidByTokenParams["_index"]) {
        return this.eth_call(functions.getBidByToken, {_tokenAddress, _tokenId, _index})
    }

    getChainId() {
        return this.eth_call(functions.getChainId, {})
    }

    getNonce(user: GetNonceParams["user"]) {
        return this.eth_call(functions.getNonce, {user})
    }

    manaToken() {
        return this.eth_call(functions.manaToken, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    paused() {
        return this.eth_call(functions.paused, {})
    }

    royaltiesCutPerMillion() {
        return this.eth_call(functions.royaltiesCutPerMillion, {})
    }

    royaltiesManager() {
        return this.eth_call(functions.royaltiesManager, {})
    }
}

/// Event types
export type BidAcceptedEventArgs = EParams<typeof events.BidAccepted>
export type BidCancelledEventArgs = EParams<typeof events.BidCancelled>
export type BidCreatedEventArgs = EParams<typeof events.BidCreated>
export type ChangedFeesCollectorCutPerMillionEventArgs = EParams<typeof events.ChangedFeesCollectorCutPerMillion>
export type ChangedRoyaltiesCutPerMillionEventArgs = EParams<typeof events.ChangedRoyaltiesCutPerMillion>
export type FeesCollectorSetEventArgs = EParams<typeof events.FeesCollectorSet>
export type MetaTransactionExecutedEventArgs = EParams<typeof events.MetaTransactionExecuted>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type PausedEventArgs = EParams<typeof events.Paused>
export type RoyaltiesManagerSetEventArgs = EParams<typeof events.RoyaltiesManagerSet>
export type UnpausedEventArgs = EParams<typeof events.Unpaused>

/// Function types
export type ERC721Composable_ValidateFingerprintParams = FunctionArguments<typeof functions.ERC721Composable_ValidateFingerprint>
export type ERC721Composable_ValidateFingerprintReturn = FunctionReturn<typeof functions.ERC721Composable_ValidateFingerprint>

export type ERC721_InterfaceParams = FunctionArguments<typeof functions.ERC721_Interface>
export type ERC721_InterfaceReturn = FunctionReturn<typeof functions.ERC721_Interface>

export type ERC721_ReceivedParams = FunctionArguments<typeof functions.ERC721_Received>
export type ERC721_ReceivedReturn = FunctionReturn<typeof functions.ERC721_Received>

export type MAX_BID_DURATIONParams = FunctionArguments<typeof functions.MAX_BID_DURATION>
export type MAX_BID_DURATIONReturn = FunctionReturn<typeof functions.MAX_BID_DURATION>

export type MIN_BID_DURATIONParams = FunctionArguments<typeof functions.MIN_BID_DURATION>
export type MIN_BID_DURATIONReturn = FunctionReturn<typeof functions.MIN_BID_DURATION>

export type ONE_MILLIONParams = FunctionArguments<typeof functions.ONE_MILLION>
export type ONE_MILLIONReturn = FunctionReturn<typeof functions.ONE_MILLION>

export type BidCounterByTokenParams = FunctionArguments<typeof functions.bidCounterByToken>
export type BidCounterByTokenReturn = FunctionReturn<typeof functions.bidCounterByToken>

export type BidIdByTokenAndBidderParams = FunctionArguments<typeof functions.bidIdByTokenAndBidder>
export type BidIdByTokenAndBidderReturn = FunctionReturn<typeof functions.bidIdByTokenAndBidder>

export type BidIndexByBidIdParams = FunctionArguments<typeof functions.bidIndexByBidId>
export type BidIndexByBidIdReturn = FunctionReturn<typeof functions.bidIndexByBidId>

export type CancelBidParams = FunctionArguments<typeof functions.cancelBid>
export type CancelBidReturn = FunctionReturn<typeof functions.cancelBid>

export type DomainSeparatorParams = FunctionArguments<typeof functions.domainSeparator>
export type DomainSeparatorReturn = FunctionReturn<typeof functions.domainSeparator>

export type ExecuteMetaTransactionParams = FunctionArguments<typeof functions.executeMetaTransaction>
export type ExecuteMetaTransactionReturn = FunctionReturn<typeof functions.executeMetaTransaction>

export type FeesCollectorParams = FunctionArguments<typeof functions.feesCollector>
export type FeesCollectorReturn = FunctionReturn<typeof functions.feesCollector>

export type FeesCollectorCutPerMillionParams = FunctionArguments<typeof functions.feesCollectorCutPerMillion>
export type FeesCollectorCutPerMillionReturn = FunctionReturn<typeof functions.feesCollectorCutPerMillion>

export type GetBidByBidderParams = FunctionArguments<typeof functions.getBidByBidder>
export type GetBidByBidderReturn = FunctionReturn<typeof functions.getBidByBidder>

export type GetBidByTokenParams = FunctionArguments<typeof functions.getBidByToken>
export type GetBidByTokenReturn = FunctionReturn<typeof functions.getBidByToken>

export type GetChainIdParams = FunctionArguments<typeof functions.getChainId>
export type GetChainIdReturn = FunctionReturn<typeof functions.getChainId>

export type GetNonceParams = FunctionArguments<typeof functions.getNonce>
export type GetNonceReturn = FunctionReturn<typeof functions.getNonce>

export type ManaTokenParams = FunctionArguments<typeof functions.manaToken>
export type ManaTokenReturn = FunctionReturn<typeof functions.manaToken>

export type OnERC721ReceivedParams = FunctionArguments<typeof functions.onERC721Received>
export type OnERC721ReceivedReturn = FunctionReturn<typeof functions.onERC721Received>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type PauseParams = FunctionArguments<typeof functions.pause>
export type PauseReturn = FunctionReturn<typeof functions.pause>

export type PausedParams = FunctionArguments<typeof functions.paused>
export type PausedReturn = FunctionReturn<typeof functions.paused>

export type PlaceBidParams_0 = FunctionArguments<typeof functions['placeBid(address,uint256,uint256,uint256)']>
export type PlaceBidReturn_0 = FunctionReturn<typeof functions['placeBid(address,uint256,uint256,uint256)']>

export type PlaceBidParams_1 = FunctionArguments<typeof functions['placeBid(address,uint256,uint256,uint256,bytes)']>
export type PlaceBidReturn_1 = FunctionReturn<typeof functions['placeBid(address,uint256,uint256,uint256,bytes)']>

export type RemoveExpiredBidsParams = FunctionArguments<typeof functions.removeExpiredBids>
export type RemoveExpiredBidsReturn = FunctionReturn<typeof functions.removeExpiredBids>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type RoyaltiesCutPerMillionParams = FunctionArguments<typeof functions.royaltiesCutPerMillion>
export type RoyaltiesCutPerMillionReturn = FunctionReturn<typeof functions.royaltiesCutPerMillion>

export type RoyaltiesManagerParams = FunctionArguments<typeof functions.royaltiesManager>
export type RoyaltiesManagerReturn = FunctionReturn<typeof functions.royaltiesManager>

export type SetFeesCollectorParams = FunctionArguments<typeof functions.setFeesCollector>
export type SetFeesCollectorReturn = FunctionReturn<typeof functions.setFeesCollector>

export type SetFeesCollectorCutPerMillionParams = FunctionArguments<typeof functions.setFeesCollectorCutPerMillion>
export type SetFeesCollectorCutPerMillionReturn = FunctionReturn<typeof functions.setFeesCollectorCutPerMillion>

export type SetRoyaltiesCutPerMillionParams = FunctionArguments<typeof functions.setRoyaltiesCutPerMillion>
export type SetRoyaltiesCutPerMillionReturn = FunctionReturn<typeof functions.setRoyaltiesCutPerMillion>

export type SetRoyaltiesManagerParams = FunctionArguments<typeof functions.setRoyaltiesManager>
export type SetRoyaltiesManagerReturn = FunctionReturn<typeof functions.setRoyaltiesManager>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

