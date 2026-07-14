import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    FeesCollected: event("0x9bcb6d1f38f6800906185471a11ede9a8e16200853225aa62558db6076490f2d", "FeesCollected(address,address,uint256)", {"feeCollector": indexed(p.address), "token": indexed(p.address), "amount": indexed(p.uint256)}),
    Initialized: event("0xc7f505b2f371ae2175ee4913f4499e1f2633a7b5936321eed1cdaeb6115181d2", "Initialized(uint64)", {"version": p.uint64}),
    OrderCreated: event("0x181de28643611afcf1cb4c095a1ef99c157e78437294f478c978e4a56e1ca77e", "OrderCreated(bytes32,(address,address,address,address,address,uint256,uint256,uint256,uint256,uint256,uint256,bytes32))", {"orderHash": indexed(p.bytes32), "order": p.struct({"fromAddress": p.address, "toAddress": p.address, "filler": p.address, "fromToken": p.address, "toToken": p.address, "expiry": p.uint256, "fromAmount": p.uint256, "fillAmount": p.uint256, "feeRate": p.uint256, "fromChain": p.uint256, "toChain": p.uint256, "postHookHash": p.bytes32})}),
    OrderFilled: event("0x6955fd9b2a7639a9baac024897cad7007b45ffa74cbfe9582d58401ff6b977b7", "OrderFilled(bytes32,(address,address,address,address,address,uint256,uint256,uint256,uint256,uint256,uint256,bytes32))", {"orderHash": indexed(p.bytes32), "order": p.struct({"fromAddress": p.address, "toAddress": p.address, "filler": p.address, "fromToken": p.address, "toToken": p.address, "expiry": p.uint256, "fromAmount": p.uint256, "fillAmount": p.uint256, "feeRate": p.uint256, "fromChain": p.uint256, "toChain": p.uint256, "postHookHash": p.bytes32})}),
    OrderRefunded: event("0xa60671d8537ed193e567f86ddf28cf35dc67073b5ad80a2d41359cfa78db0a1e", "OrderRefunded(bytes32)", {"orderHash": indexed(p.bytes32)}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    PeerSet: event("0x238399d427b947898edb290f5ff0f9109849b1c3ba196a42e35f00c50a54b98b", "PeerSet(uint32,bytes32)", {"eid": p.uint32, "peer": p.bytes32}),
    SettlementForwarded: event("0x69f975bd70ea51b973eb6aff3812f49adf595bd59d6f3d29840d5695cc19ba30", "SettlementForwarded(bytes32)", {"orderHash": indexed(p.bytes32)}),
    SpokeInitialized: event("0xf25a5e989fb7e02dc64e8a2c85e4fbaae049d3ce88c8cbb840860122201da24b", "SpokeInitialized(address,address,address,address,string,string)", {"gateway": indexed(p.address), "gasService": indexed(p.address), "squidMulticall": p.address, "feeCollector": p.address, "hubChainName": p.string, "hubAddress": p.string}),
    TokensReleased: event("0xd48052bf92f3eec93ecdeeec72ea80e1071c926cb4d6e5a37ee71be8a0ce9a10", "TokensReleased(bytes32)", {"orderHash": indexed(p.bytes32)}),
    TrustedAddressRemoved: event("0xf9400637a329865492b8d0d4dba4eafc7e8d5d0fae5e27b56766816d2ae1b2ca", "TrustedAddressRemoved(string)", {"chain": p.string}),
    TrustedAddressSet: event("0xdb6b260ea45f7fe513e1d3b8c21017a29e3a41610e95aefb8862b81c69aec61c", "TrustedAddressSet(string,string)", {"chain": p.string, "address_": p.string}),
}

export const functions = {
    addressToBytes32: viewFun("0x82c947b7", "addressToBytes32(address)", {"_addr": p.address}, p.bytes32),
    allowInitializePath: viewFun("0xff7bd03d", "allowInitializePath((uint32,bytes32,uint64))", {"origin": p.struct({"srcEid": p.uint32, "sender": p.bytes32, "nonce": p.uint64})}, p.bool),
    chainName: viewFun("0x1c93b03a", "chainName()", {}, p.string),
    collectFees: fun("0x58c0f729", "collectFees(address[])", {"tokens": p.array(p.address)}, ),
    createOrder: fun("0x0d77797c", "createOrder((address,address,address,address,address,uint256,uint256,uint256,uint256,uint256,uint256,bytes32))", {"order": p.struct({"fromAddress": p.address, "toAddress": p.address, "filler": p.address, "fromToken": p.address, "toToken": p.address, "expiry": p.uint256, "fromAmount": p.uint256, "fillAmount": p.uint256, "feeRate": p.uint256, "fromChain": p.uint256, "toChain": p.uint256, "postHookHash": p.bytes32})}, ),
    endpoint: viewFun("0x5e280f11", "endpoint()", {}, p.address),
    execute: fun("0x49160658", "execute(bytes32,string,string,bytes)", {"commandId": p.bytes32, "sourceChain": p.string, "sourceAddress": p.string, "payload": p.bytes}, ),
    executeWithToken: fun("0x1a98b2e0", "executeWithToken(bytes32,string,string,bytes,string,uint256)", {"commandId": p.bytes32, "sourceChain": p.string, "sourceAddress": p.string, "payload": p.bytes, "tokenSymbol": p.string, "amount": p.uint256}, ),
    feeCollector: viewFun("0xc415b95c", "feeCollector()", {}, p.address),
    fillOrder: fun("0xaab59a09", "fillOrder((address,address,address,address,address,uint256,uint256,uint256,uint256,uint256,uint256,bytes32),(uint8,address,uint256,bytes,bytes)[])", {"order": p.struct({"fromAddress": p.address, "toAddress": p.address, "filler": p.address, "fromToken": p.address, "toToken": p.address, "expiry": p.uint256, "fromAmount": p.uint256, "fillAmount": p.uint256, "feeRate": p.uint256, "fromChain": p.uint256, "toChain": p.uint256, "postHookHash": p.bytes32}), "calls": p.array(p.struct({"callType": p.uint8, "target": p.address, "value": p.uint256, "callData": p.bytes, "payload": p.bytes}))}, ),
    forwardSettlements: fun("0x0630dea4", "forwardSettlements(bytes32[],uint256,uint128,uint8)", {"orderHashes": p.array(p.bytes32), "lzFee": p.uint256, "gasLimit": p.uint128, "provider": p.uint8}, ),
    gasService: viewFun("0x6a22d8cc", "gasService()", {}, p.address),
    gateway: viewFun("0x116191b6", "gateway()", {}, p.address),
    hubAddress: viewFun("0xf69b1b29", "hubAddress()", {}, p.string),
    hubAddressBytes32: viewFun("0x951f4016", "hubAddressBytes32()", {}, p.bytes32),
    hubChainName: viewFun("0x82e5f9e4", "hubChainName()", {}, p.string),
    hubEndpoint: viewFun("0x2b8cb1f6", "hubEndpoint()", {}, p.uint32),
    initialize: fun("0x0af7b75b", "initialize(address,address,address,address,string,string,address,address,address,uint32)", {"_axelarGateway": p.address, "_axelarGasService": p.address, "_squidMulticall": p.address, "_feeCollector": p.address, "_hubChainName": p.string, "_hubAddress": p.string, "_endpoint": p.address, "_owner": p.address, "_hub": p.address, "_hubEndpoint": p.uint32}, ),
    isComposeMsgSender: viewFun("0x82413eac", "isComposeMsgSender((uint32,bytes32,uint64),bytes,address)", {"_0": p.struct({"srcEid": p.uint32, "sender": p.bytes32, "nonce": p.uint64}), "_1": p.bytes, "_sender": p.address}, p.bool),
    isTrustedAddress: viewFun("0xc506bff4", "isTrustedAddress(string,string)", {"chain": p.string, "address_": p.string}, p.bool),
    lzReceive: fun("0x13137d65", "lzReceive((uint32,bytes32,uint64),bytes32,bytes,address,bytes)", {"_origin": p.struct({"srcEid": p.uint32, "sender": p.bytes32, "nonce": p.uint64}), "_guid": p.bytes32, "_message": p.bytes, "_executor": p.address, "_extraData": p.bytes}, ),
    nextNonce: viewFun("0x7d25a05e", "nextNonce(uint32,bytes32)", {"_0": p.uint32, "_1": p.bytes32}, p.uint64),
    oAppVersion: viewFun("0x17442b70", "oAppVersion()", {}, {"senderVersion": p.uint64, "receiverVersion": p.uint64}),
    orderHashToStatus: viewFun("0x076e9f6c", "orderHashToStatus(bytes32)", {"_0": p.bytes32}, p.uint8),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    peers: viewFun("0xbb0b6a53", "peers(uint32)", {"eid": p.uint32}, p.bytes32),
    quote: viewFun("0x21b4ae78", "quote(uint32,bytes32[],uint128,bool)", {"_dstEid": p.uint32, "orderHashes": p.array(p.bytes32), "gasLimit": p.uint128, "_payInLzToken": p.bool}, {"nativeFee": p.uint256, "lzTokenFee": p.uint256}),
    refundOrder: fun("0x1e44fb97", "refundOrder((address,address,address,address,address,uint256,uint256,uint256,uint256,uint256,uint256,bytes32))", {"order": p.struct({"fromAddress": p.address, "toAddress": p.address, "filler": p.address, "fromToken": p.address, "toToken": p.address, "expiry": p.uint256, "fromAmount": p.uint256, "fillAmount": p.uint256, "feeRate": p.uint256, "fromChain": p.uint256, "toChain": p.uint256, "postHookHash": p.bytes32})}, ),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    setDelegate: fun("0xca5eb5e1", "setDelegate(address)", {"_delegate": p.address}, ),
    setPeer: fun("0x3400288b", "setPeer(uint32,bytes32)", {"_eid": p.uint32, "_peer": p.bytes32}, ),
    settlementToStatus: viewFun("0xa32b52a7", "settlementToStatus(bytes32)", {"_0": p.bytes32}, p.uint8),
    squidMulticall: viewFun("0x59ce62e9", "squidMulticall()", {}, p.address),
    tokenToCollectedFees: viewFun("0xdb715f7b", "tokenToCollectedFees(address)", {"_0": p.address}, p.uint256),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
    trustedAddress: viewFun("0x477aedc7", "trustedAddress(string)", {"chain": p.string}, p.string),
    trustedAddressHash: viewFun("0xffd5982a", "trustedAddressHash(string)", {"chain": p.string}, p.bytes32),
}

export class Contract extends ContractBase {

    addressToBytes32(_addr: AddressToBytes32Params["_addr"]) {
        return this.eth_call(functions.addressToBytes32, {_addr})
    }

    allowInitializePath(origin: AllowInitializePathParams["origin"]) {
        return this.eth_call(functions.allowInitializePath, {origin})
    }

    chainName() {
        return this.eth_call(functions.chainName, {})
    }

    endpoint() {
        return this.eth_call(functions.endpoint, {})
    }

    feeCollector() {
        return this.eth_call(functions.feeCollector, {})
    }

    gasService() {
        return this.eth_call(functions.gasService, {})
    }

    gateway() {
        return this.eth_call(functions.gateway, {})
    }

    hubAddress() {
        return this.eth_call(functions.hubAddress, {})
    }

    hubAddressBytes32() {
        return this.eth_call(functions.hubAddressBytes32, {})
    }

    hubChainName() {
        return this.eth_call(functions.hubChainName, {})
    }

    hubEndpoint() {
        return this.eth_call(functions.hubEndpoint, {})
    }

    isComposeMsgSender(_0: IsComposeMsgSenderParams["_0"], _1: IsComposeMsgSenderParams["_1"], _sender: IsComposeMsgSenderParams["_sender"]) {
        return this.eth_call(functions.isComposeMsgSender, {_0, _1, _sender})
    }

    isTrustedAddress(chain: IsTrustedAddressParams["chain"], address_: IsTrustedAddressParams["address_"]) {
        return this.eth_call(functions.isTrustedAddress, {chain, address_})
    }

    nextNonce(_0: NextNonceParams["_0"], _1: NextNonceParams["_1"]) {
        return this.eth_call(functions.nextNonce, {_0, _1})
    }

    oAppVersion() {
        return this.eth_call(functions.oAppVersion, {})
    }

    orderHashToStatus(_0: OrderHashToStatusParams["_0"]) {
        return this.eth_call(functions.orderHashToStatus, {_0})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    peers(eid: PeersParams["eid"]) {
        return this.eth_call(functions.peers, {eid})
    }

    quote(_dstEid: QuoteParams["_dstEid"], orderHashes: QuoteParams["orderHashes"], gasLimit: QuoteParams["gasLimit"], _payInLzToken: QuoteParams["_payInLzToken"]) {
        return this.eth_call(functions.quote, {_dstEid, orderHashes, gasLimit, _payInLzToken})
    }

    settlementToStatus(_0: SettlementToStatusParams["_0"]) {
        return this.eth_call(functions.settlementToStatus, {_0})
    }

    squidMulticall() {
        return this.eth_call(functions.squidMulticall, {})
    }

    tokenToCollectedFees(_0: TokenToCollectedFeesParams["_0"]) {
        return this.eth_call(functions.tokenToCollectedFees, {_0})
    }

    trustedAddress(chain: TrustedAddressParams["chain"]) {
        return this.eth_call(functions.trustedAddress, {chain})
    }

    trustedAddressHash(chain: TrustedAddressHashParams["chain"]) {
        return this.eth_call(functions.trustedAddressHash, {chain})
    }
}

/// Event types
export type FeesCollectedEventArgs = EParams<typeof events.FeesCollected>
export type InitializedEventArgs = EParams<typeof events.Initialized>
export type OrderCreatedEventArgs = EParams<typeof events.OrderCreated>
export type OrderFilledEventArgs = EParams<typeof events.OrderFilled>
export type OrderRefundedEventArgs = EParams<typeof events.OrderRefunded>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type PeerSetEventArgs = EParams<typeof events.PeerSet>
export type SettlementForwardedEventArgs = EParams<typeof events.SettlementForwarded>
export type SpokeInitializedEventArgs = EParams<typeof events.SpokeInitialized>
export type TokensReleasedEventArgs = EParams<typeof events.TokensReleased>
export type TrustedAddressRemovedEventArgs = EParams<typeof events.TrustedAddressRemoved>
export type TrustedAddressSetEventArgs = EParams<typeof events.TrustedAddressSet>

/// Function types
export type AddressToBytes32Params = FunctionArguments<typeof functions.addressToBytes32>
export type AddressToBytes32Return = FunctionReturn<typeof functions.addressToBytes32>

export type AllowInitializePathParams = FunctionArguments<typeof functions.allowInitializePath>
export type AllowInitializePathReturn = FunctionReturn<typeof functions.allowInitializePath>

export type ChainNameParams = FunctionArguments<typeof functions.chainName>
export type ChainNameReturn = FunctionReturn<typeof functions.chainName>

export type CollectFeesParams = FunctionArguments<typeof functions.collectFees>
export type CollectFeesReturn = FunctionReturn<typeof functions.collectFees>

export type CreateOrderParams = FunctionArguments<typeof functions.createOrder>
export type CreateOrderReturn = FunctionReturn<typeof functions.createOrder>

export type EndpointParams = FunctionArguments<typeof functions.endpoint>
export type EndpointReturn = FunctionReturn<typeof functions.endpoint>

export type ExecuteParams = FunctionArguments<typeof functions.execute>
export type ExecuteReturn = FunctionReturn<typeof functions.execute>

export type ExecuteWithTokenParams = FunctionArguments<typeof functions.executeWithToken>
export type ExecuteWithTokenReturn = FunctionReturn<typeof functions.executeWithToken>

export type FeeCollectorParams = FunctionArguments<typeof functions.feeCollector>
export type FeeCollectorReturn = FunctionReturn<typeof functions.feeCollector>

export type FillOrderParams = FunctionArguments<typeof functions.fillOrder>
export type FillOrderReturn = FunctionReturn<typeof functions.fillOrder>

export type ForwardSettlementsParams = FunctionArguments<typeof functions.forwardSettlements>
export type ForwardSettlementsReturn = FunctionReturn<typeof functions.forwardSettlements>

export type GasServiceParams = FunctionArguments<typeof functions.gasService>
export type GasServiceReturn = FunctionReturn<typeof functions.gasService>

export type GatewayParams = FunctionArguments<typeof functions.gateway>
export type GatewayReturn = FunctionReturn<typeof functions.gateway>

export type HubAddressParams = FunctionArguments<typeof functions.hubAddress>
export type HubAddressReturn = FunctionReturn<typeof functions.hubAddress>

export type HubAddressBytes32Params = FunctionArguments<typeof functions.hubAddressBytes32>
export type HubAddressBytes32Return = FunctionReturn<typeof functions.hubAddressBytes32>

export type HubChainNameParams = FunctionArguments<typeof functions.hubChainName>
export type HubChainNameReturn = FunctionReturn<typeof functions.hubChainName>

export type HubEndpointParams = FunctionArguments<typeof functions.hubEndpoint>
export type HubEndpointReturn = FunctionReturn<typeof functions.hubEndpoint>

export type InitializeParams = FunctionArguments<typeof functions.initialize>
export type InitializeReturn = FunctionReturn<typeof functions.initialize>

export type IsComposeMsgSenderParams = FunctionArguments<typeof functions.isComposeMsgSender>
export type IsComposeMsgSenderReturn = FunctionReturn<typeof functions.isComposeMsgSender>

export type IsTrustedAddressParams = FunctionArguments<typeof functions.isTrustedAddress>
export type IsTrustedAddressReturn = FunctionReturn<typeof functions.isTrustedAddress>

export type LzReceiveParams = FunctionArguments<typeof functions.lzReceive>
export type LzReceiveReturn = FunctionReturn<typeof functions.lzReceive>

export type NextNonceParams = FunctionArguments<typeof functions.nextNonce>
export type NextNonceReturn = FunctionReturn<typeof functions.nextNonce>

export type OAppVersionParams = FunctionArguments<typeof functions.oAppVersion>
export type OAppVersionReturn = FunctionReturn<typeof functions.oAppVersion>

export type OrderHashToStatusParams = FunctionArguments<typeof functions.orderHashToStatus>
export type OrderHashToStatusReturn = FunctionReturn<typeof functions.orderHashToStatus>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type PeersParams = FunctionArguments<typeof functions.peers>
export type PeersReturn = FunctionReturn<typeof functions.peers>

export type QuoteParams = FunctionArguments<typeof functions.quote>
export type QuoteReturn = FunctionReturn<typeof functions.quote>

export type RefundOrderParams = FunctionArguments<typeof functions.refundOrder>
export type RefundOrderReturn = FunctionReturn<typeof functions.refundOrder>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type SetDelegateParams = FunctionArguments<typeof functions.setDelegate>
export type SetDelegateReturn = FunctionReturn<typeof functions.setDelegate>

export type SetPeerParams = FunctionArguments<typeof functions.setPeer>
export type SetPeerReturn = FunctionReturn<typeof functions.setPeer>

export type SettlementToStatusParams = FunctionArguments<typeof functions.settlementToStatus>
export type SettlementToStatusReturn = FunctionReturn<typeof functions.settlementToStatus>

export type SquidMulticallParams = FunctionArguments<typeof functions.squidMulticall>
export type SquidMulticallReturn = FunctionReturn<typeof functions.squidMulticall>

export type TokenToCollectedFeesParams = FunctionArguments<typeof functions.tokenToCollectedFees>
export type TokenToCollectedFeesReturn = FunctionReturn<typeof functions.tokenToCollectedFees>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type TrustedAddressParams = FunctionArguments<typeof functions.trustedAddress>
export type TrustedAddressReturn = FunctionReturn<typeof functions.trustedAddress>

export type TrustedAddressHashParams = FunctionArguments<typeof functions.trustedAddressHash>
export type TrustedAddressHashReturn = FunctionReturn<typeof functions.trustedAddressHash>

