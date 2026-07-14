import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    BidCreated: event("0xe45b7709f1eed66d6e28f43b32f2003da0f0c40b92f75a8994370516e048620f", "BidCreated(bytes32,address,uint256,address,uint256,uint256,bytes)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": indexed(p.address), "_price": p.uint256, "_expiresAt": p.uint256, "_fingerprint": p.bytes}),
    BidAccepted: event("0x4e5ca6c6c06fa8d62f2930830e0d370de70f108bd89213de0b51141775e695bd", "BidAccepted(bytes32,address,uint256,address,address,uint256,uint256)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": p.address, "_seller": indexed(p.address), "_price": p.uint256, "_fee": p.uint256}),
    BidCancelled: event("0xc43098075c34b8b92567bd49f08f55e397ebf82dd82072e3eb1be525c4506f5f", "BidCancelled(bytes32,address,uint256,address)", {"_id": p.bytes32, "_tokenAddress": indexed(p.address), "_tokenId": indexed(p.uint256), "_bidder": indexed(p.address)}),
    ChangedOwnerCutPerMillion: event("0xfa406a120a9e7f2b332bfb7a43d3bf1c3f079262202907a6b69c94b2821a02c6", "ChangedOwnerCutPerMillion(uint256)", {"_ownerCutPerMillion": p.uint256}),
    Paused: event("0x62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258", "Paused(address)", {"account": p.address}),
    Unpaused: event("0x5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa", "Unpaused(address)", {"account": p.address}),
    PauserAdded: event("0x6719d08c1888103bea251a4ed56406bd0c3e69723c8a1686e017e7bbe159b6f8", "PauserAdded(address)", {"account": indexed(p.address)}),
    PauserRemoved: event("0xcd265ebaf09df2871cc7bd4133404a235ba12eff2041bb89d9c714a2621c7c7e", "PauserRemoved(address)", {"account": indexed(p.address)}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
}

export const functions = {
    getBidByBidder: viewFun("0x00f052f6", "getBidByBidder(address,uint256,address)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_bidder": p.address}, {"bidIndex": p.uint256, "bidId": p.bytes32, "bidder": p.address, "price": p.uint256, "expiresAt": p.uint256}),
    ERC721Composable_ValidateFingerprint: viewFun("0x12712f7c", "ERC721Composable_ValidateFingerprint()", {}, p.bytes4),
    onERC721Received: fun("0x150b7a02", "onERC721Received(address,address,uint256,bytes)", {"_from": p.address, "_1": p.address, "_tokenId": p.uint256, "_data": p.bytes}, p.bytes4),
    setOwnerCutPerMillion: fun("0x19dad16d", "setOwnerCutPerMillion(uint256)", {"_ownerCutPerMillion": p.uint256}, ),
    bidIdByTokenAndBidder: viewFun("0x279dc1e4", "bidIdByTokenAndBidder(address,uint256,address)", {"_0": p.address, "_1": p.uint256, "_2": p.address}, p.bytes32),
    bidCounterByToken: viewFun("0x28bd196a", "bidCounterByToken(address,uint256)", {"_0": p.address, "_1": p.uint256}, p.uint256),
    ERC721_Interface: viewFun("0x2b4c32be", "ERC721_Interface()", {}, p.bytes4),
    cancelBid: fun("0x39b6b1e5", "cancelBid(address,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256}, ),
    unpause: fun("0x3f4ba83a", "unpause()", {}, ),
    getBidByToken: viewFun("0x42a6c4dd", "getBidByToken(address,uint256,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_index": p.uint256}, {"_0": p.bytes32, "_1": p.address, "_2": p.uint256, "_3": p.uint256}),
    isPauser: viewFun("0x46fbf68e", "isPauser(address)", {"account": p.address}, p.bool),
    ERC721_Received: viewFun("0x4c81a727", "ERC721_Received()", {}, p.bytes4),
    paused: viewFun("0x5c975abb", "paused()", {}, p.bool),
    renouncePauser: fun("0x6ef8d66d", "renouncePauser()", {}, ),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    manaToken: viewFun("0x74c97c99", "manaToken()", {}, p.address),
    'placeBid(address,uint256,uint256,uint256)': fun("0x81281be8", "placeBid(address,uint256,uint256,uint256)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_price": p.uint256, "_duration": p.uint256}, ),
    addPauser: fun("0x82dc1ec4", "addPauser(address)", {"account": p.address}, ),
    pause: fun("0x8456cb59", "pause()", {}, ),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    isOwner: viewFun("0x8f32d59b", "isOwner()", {}, p.bool),
    ONE_MILLION: viewFun("0x9869b736", "ONE_MILLION()", {}, p.uint256),
    ownerCutPerMillion: viewFun("0xa01f79d4", "ownerCutPerMillion()", {}, p.uint256),
    MAX_BID_DURATION: viewFun("0xa750066e", "MAX_BID_DURATION()", {}, p.uint256),
    'placeBid(address,uint256,uint256,uint256,bytes)': fun("0xac063725", "placeBid(address,uint256,uint256,uint256,bytes)", {"_tokenAddress": p.address, "_tokenId": p.uint256, "_price": p.uint256, "_duration": p.uint256, "_fingerprint": p.bytes}, ),
    bidIndexByBidId: viewFun("0xb5eea644", "bidIndexByBidId(bytes32)", {"_0": p.bytes32}, p.uint256),
    removeExpiredBids: fun("0xce1056cb", "removeExpiredBids(address[],uint256[],address[])", {"_tokenAddresses": p.array(p.address), "_tokenIds": p.array(p.uint256), "_bidders": p.array(p.address)}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
    MIN_BID_DURATION: viewFun("0xf88a1217", "MIN_BID_DURATION()", {}, p.uint256),
}

export class Contract extends ContractBase {

    getBidByBidder(_tokenAddress: GetBidByBidderParams["_tokenAddress"], _tokenId: GetBidByBidderParams["_tokenId"], _bidder: GetBidByBidderParams["_bidder"]) {
        return this.eth_call(functions.getBidByBidder, {_tokenAddress, _tokenId, _bidder})
    }

    ERC721Composable_ValidateFingerprint() {
        return this.eth_call(functions.ERC721Composable_ValidateFingerprint, {})
    }

    bidIdByTokenAndBidder(_0: BidIdByTokenAndBidderParams["_0"], _1: BidIdByTokenAndBidderParams["_1"], _2: BidIdByTokenAndBidderParams["_2"]) {
        return this.eth_call(functions.bidIdByTokenAndBidder, {_0, _1, _2})
    }

    bidCounterByToken(_0: BidCounterByTokenParams["_0"], _1: BidCounterByTokenParams["_1"]) {
        return this.eth_call(functions.bidCounterByToken, {_0, _1})
    }

    ERC721_Interface() {
        return this.eth_call(functions.ERC721_Interface, {})
    }

    getBidByToken(_tokenAddress: GetBidByTokenParams["_tokenAddress"], _tokenId: GetBidByTokenParams["_tokenId"], _index: GetBidByTokenParams["_index"]) {
        return this.eth_call(functions.getBidByToken, {_tokenAddress, _tokenId, _index})
    }

    isPauser(account: IsPauserParams["account"]) {
        return this.eth_call(functions.isPauser, {account})
    }

    ERC721_Received() {
        return this.eth_call(functions.ERC721_Received, {})
    }

    paused() {
        return this.eth_call(functions.paused, {})
    }

    manaToken() {
        return this.eth_call(functions.manaToken, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    isOwner() {
        return this.eth_call(functions.isOwner, {})
    }

    ONE_MILLION() {
        return this.eth_call(functions.ONE_MILLION, {})
    }

    ownerCutPerMillion() {
        return this.eth_call(functions.ownerCutPerMillion, {})
    }

    MAX_BID_DURATION() {
        return this.eth_call(functions.MAX_BID_DURATION, {})
    }

    bidIndexByBidId(_0: BidIndexByBidIdParams["_0"]) {
        return this.eth_call(functions.bidIndexByBidId, {_0})
    }

    MIN_BID_DURATION() {
        return this.eth_call(functions.MIN_BID_DURATION, {})
    }
}

/// Event types
export type BidCreatedEventArgs = EParams<typeof events.BidCreated>
export type BidAcceptedEventArgs = EParams<typeof events.BidAccepted>
export type BidCancelledEventArgs = EParams<typeof events.BidCancelled>
export type ChangedOwnerCutPerMillionEventArgs = EParams<typeof events.ChangedOwnerCutPerMillion>
export type PausedEventArgs = EParams<typeof events.Paused>
export type UnpausedEventArgs = EParams<typeof events.Unpaused>
export type PauserAddedEventArgs = EParams<typeof events.PauserAdded>
export type PauserRemovedEventArgs = EParams<typeof events.PauserRemoved>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>

/// Function types
export type GetBidByBidderParams = FunctionArguments<typeof functions.getBidByBidder>
export type GetBidByBidderReturn = FunctionReturn<typeof functions.getBidByBidder>

export type ERC721Composable_ValidateFingerprintParams = FunctionArguments<typeof functions.ERC721Composable_ValidateFingerprint>
export type ERC721Composable_ValidateFingerprintReturn = FunctionReturn<typeof functions.ERC721Composable_ValidateFingerprint>

export type OnERC721ReceivedParams = FunctionArguments<typeof functions.onERC721Received>
export type OnERC721ReceivedReturn = FunctionReturn<typeof functions.onERC721Received>

export type SetOwnerCutPerMillionParams = FunctionArguments<typeof functions.setOwnerCutPerMillion>
export type SetOwnerCutPerMillionReturn = FunctionReturn<typeof functions.setOwnerCutPerMillion>

export type BidIdByTokenAndBidderParams = FunctionArguments<typeof functions.bidIdByTokenAndBidder>
export type BidIdByTokenAndBidderReturn = FunctionReturn<typeof functions.bidIdByTokenAndBidder>

export type BidCounterByTokenParams = FunctionArguments<typeof functions.bidCounterByToken>
export type BidCounterByTokenReturn = FunctionReturn<typeof functions.bidCounterByToken>

export type ERC721_InterfaceParams = FunctionArguments<typeof functions.ERC721_Interface>
export type ERC721_InterfaceReturn = FunctionReturn<typeof functions.ERC721_Interface>

export type CancelBidParams = FunctionArguments<typeof functions.cancelBid>
export type CancelBidReturn = FunctionReturn<typeof functions.cancelBid>

export type UnpauseParams = FunctionArguments<typeof functions.unpause>
export type UnpauseReturn = FunctionReturn<typeof functions.unpause>

export type GetBidByTokenParams = FunctionArguments<typeof functions.getBidByToken>
export type GetBidByTokenReturn = FunctionReturn<typeof functions.getBidByToken>

export type IsPauserParams = FunctionArguments<typeof functions.isPauser>
export type IsPauserReturn = FunctionReturn<typeof functions.isPauser>

export type ERC721_ReceivedParams = FunctionArguments<typeof functions.ERC721_Received>
export type ERC721_ReceivedReturn = FunctionReturn<typeof functions.ERC721_Received>

export type PausedParams = FunctionArguments<typeof functions.paused>
export type PausedReturn = FunctionReturn<typeof functions.paused>

export type RenouncePauserParams = FunctionArguments<typeof functions.renouncePauser>
export type RenouncePauserReturn = FunctionReturn<typeof functions.renouncePauser>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type ManaTokenParams = FunctionArguments<typeof functions.manaToken>
export type ManaTokenReturn = FunctionReturn<typeof functions.manaToken>

export type PlaceBidParams_0 = FunctionArguments<typeof functions['placeBid(address,uint256,uint256,uint256)']>
export type PlaceBidReturn_0 = FunctionReturn<typeof functions['placeBid(address,uint256,uint256,uint256)']>

export type AddPauserParams = FunctionArguments<typeof functions.addPauser>
export type AddPauserReturn = FunctionReturn<typeof functions.addPauser>

export type PauseParams = FunctionArguments<typeof functions.pause>
export type PauseReturn = FunctionReturn<typeof functions.pause>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type IsOwnerParams = FunctionArguments<typeof functions.isOwner>
export type IsOwnerReturn = FunctionReturn<typeof functions.isOwner>

export type ONE_MILLIONParams = FunctionArguments<typeof functions.ONE_MILLION>
export type ONE_MILLIONReturn = FunctionReturn<typeof functions.ONE_MILLION>

export type OwnerCutPerMillionParams = FunctionArguments<typeof functions.ownerCutPerMillion>
export type OwnerCutPerMillionReturn = FunctionReturn<typeof functions.ownerCutPerMillion>

export type MAX_BID_DURATIONParams = FunctionArguments<typeof functions.MAX_BID_DURATION>
export type MAX_BID_DURATIONReturn = FunctionReturn<typeof functions.MAX_BID_DURATION>

export type PlaceBidParams_1 = FunctionArguments<typeof functions['placeBid(address,uint256,uint256,uint256,bytes)']>
export type PlaceBidReturn_1 = FunctionReturn<typeof functions['placeBid(address,uint256,uint256,uint256,bytes)']>

export type BidIndexByBidIdParams = FunctionArguments<typeof functions.bidIndexByBidId>
export type BidIndexByBidIdReturn = FunctionReturn<typeof functions.bidIndexByBidId>

export type RemoveExpiredBidsParams = FunctionArguments<typeof functions.removeExpiredBids>
export type RemoveExpiredBidsReturn = FunctionReturn<typeof functions.removeExpiredBids>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type MIN_BID_DURATIONParams = FunctionArguments<typeof functions.MIN_BID_DURATION>
export type MIN_BID_DURATIONReturn = FunctionReturn<typeof functions.MIN_BID_DURATION>

