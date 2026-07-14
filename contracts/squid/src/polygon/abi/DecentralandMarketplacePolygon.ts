import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    ContractSignatureIndexIncreased: event("0x09cc1ddfeaa3955efcd7527dadf166c8dd69e1e1ac87e52919eb59da0b42ee19", "ContractSignatureIndexIncreased(address,uint256)", {"_caller": indexed(p.address), "_newValue": indexed(p.uint256)}),
    CouponManagerUpdated: event("0x143fb5dd4c4edb29ec8dd5b4bd1d0c1fbdeec8869d2a3548e922affb61470f82", "CouponManagerUpdated(address,address)", {"_caller": indexed(p.address), "_couponManager": indexed(p.address)}),
    FeeCollectorUpdated: event("0x5d16ad41baeb009cd23eb8f6c7cde5c2e0cd5acf4a33926ab488875c37c37f38", "FeeCollectorUpdated(address,address)", {"_caller": indexed(p.address), "_feeCollector": indexed(p.address)}),
    FeeRateUpdated: event("0x98259702e6263eb2c9423b892e36fcaaaac8885fbeab7826218791df24d84987", "FeeRateUpdated(address,uint256)", {"_caller": indexed(p.address), "_feeRate": p.uint256}),
    ManaUsdAggregatorUpdated: event("0xc8bc0ccd477b6e4af64bb15c131d3d3540180dfa65e494b1b3a9291a9d24b754", "ManaUsdAggregatorUpdated(address,uint256)", {"_aggregator": indexed(p.address), "_tolerance": p.uint256}),
    MetaTransactionExecuted: event("0x5845892132946850460bff5a0083f71031bc5bf9aadcd40f1de79423eac9b10b", "MetaTransactionExecuted(address,address,bytes)", {"_userAddress": indexed(p.address), "_relayerAddress": indexed(p.address), "_functionData": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    Paused: event("0x62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258", "Paused(address)", {"account": p.address}),
    RoyaltiesManagerUpdated: event("0x77d1457fe352712d480bd254327263c64ce777ddfc62c1a0eda5bf2bd843ca2f", "RoyaltiesManagerUpdated(address,address)", {"_caller": indexed(p.address), "_royaltiesManager": indexed(p.address)}),
    RoyaltiesRateUpdated: event("0xba55b878cbf32ed69c9c62c58b9c5ec31d1271d9510d2b08a1f9091236d8eacf", "RoyaltiesRateUpdated(address,uint256)", {"_caller": indexed(p.address), "_royaltiesRate": p.uint256}),
    SignatureCancelled: event("0x8c4e82d0c2e3a462d0450cd1797aef55b85987a833dd3d93b61d96824a01410e", "SignatureCancelled(address,bytes32)", {"_caller": indexed(p.address), "_signature": indexed(p.bytes32)}),
    SignerSignatureIndexIncreased: event("0x492ddec6f62ab9180c9800a9a60ec15bb0dbae20d2a19cdc824448872f796667", "SignerSignatureIndexIncreased(address,uint256)", {"_caller": indexed(p.address), "_newValue": indexed(p.uint256)}),
    Traded: event("0xaaecdfa7e74e704650fcb273f630f42f68974eff42bfffc1732cf30db9e4685b", "Traded(address,bytes32,(address,bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,uint256,address,bytes)[]))", {"_caller": indexed(p.address), "_signature": indexed(p.bytes32), "_trade": p.struct({"signer": p.address, "signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "sent": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes})), "received": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes}))})}),
    Unpaused: event("0x5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa", "Unpaused(address)", {"account": p.address}),
}

export const functions = {
    ASSET_TYPE_COLLECTION_ITEM: viewFun("0xba9ea7bd", "ASSET_TYPE_COLLECTION_ITEM()", {}, p.uint256),
    ASSET_TYPE_ERC20: viewFun("0x6c1d8ee6", "ASSET_TYPE_ERC20()", {}, p.uint256),
    ASSET_TYPE_ERC721: viewFun("0x7bb185bd", "ASSET_TYPE_ERC721()", {}, p.uint256),
    ASSET_TYPE_USD_PEGGED_MANA: viewFun("0xa5b2a654", "ASSET_TYPE_USD_PEGGED_MANA()", {}, p.uint256),
    accept: fun("0x961a547e", "accept((address,bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,uint256,address,bytes)[])[])", {"_trades": p.array(p.struct({"signer": p.address, "signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "sent": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes})), "received": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes}))}))}, ),
    acceptWithCoupon: fun("0xad1e8700", "acceptWithCoupon((address,bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,uint256,address,bytes)[])[],(bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),address,bytes,bytes)[])", {"_trades": p.array(p.struct({"signer": p.address, "signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "sent": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes})), "received": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes}))})), "_coupons": p.array(p.struct({"signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "couponAddress": p.address, "data": p.bytes, "callerData": p.bytes}))}, ),
    cancelSignature: fun("0x6f773280", "cancelSignature((address,bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,uint256,address,bytes)[])[])", {"_trades": p.array(p.struct({"signer": p.address, "signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "sent": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes})), "received": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes}))}))}, ),
    cancelledSignatures: viewFun("0xb74f4404", "cancelledSignatures(bytes32)", {"_0": p.bytes32}, p.bool),
    contractSignatureIndex: viewFun("0x2175b473", "contractSignatureIndex()", {}, p.uint256),
    couponManager: viewFun("0x6cb5b426", "couponManager()", {}, p.address),
    executeMetaTransaction: fun("0xd8ed1acc", "executeMetaTransaction(address,bytes,bytes)", {"_userAddress": p.address, "_functionData": p.bytes, "_signature": p.bytes}, p.bytes),
    feeCollector: viewFun("0xc415b95c", "feeCollector()", {}, p.address),
    feeRate: viewFun("0x978bbdb9", "feeRate()", {}, p.uint256),
    getNonce: viewFun("0x2d0335ab", "getNonce(address)", {"_signer": p.address}, p.uint256),
    getTradeId: viewFun("0x0d65630a", "getTradeId((address,bytes,(uint256,uint256,uint256,bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,uint256,address,bytes)[]),address)", {"_trade": p.struct({"signer": p.address, "signature": p.bytes, "checks": p.struct({"uses": p.uint256, "expiration": p.uint256, "effective": p.uint256, "salt": p.bytes32, "contractSignatureIndex": p.uint256, "signerSignatureIndex": p.uint256, "allowedRoot": p.bytes32, "allowedProof": p.array(p.bytes32), "externalChecks": p.array(p.struct({"contractAddress": p.address, "selector": p.bytes4, "value": p.bytes, "required": p.bool}))}), "sent": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes})), "received": p.array(p.struct({"assetType": p.uint256, "contractAddress": p.address, "value": p.uint256, "beneficiary": p.address, "extra": p.bytes}))}), "_caller": p.address}, p.bytes32),
    increaseContractSignatureIndex: fun("0xb861cd19", "increaseContractSignatureIndex()", {}, ),
    increaseSignerSignatureIndex: fun("0xceefad9f", "increaseSignerSignatureIndex()", {}, ),
    manaAddress: viewFun("0xb9e66a26", "manaAddress()", {}, p.address),
    manaUsdAggregator: viewFun("0x05dec3e4", "manaUsdAggregator()", {}, p.address),
    manaUsdAggregatorTolerance: viewFun("0x79fc633f", "manaUsdAggregatorTolerance()", {}, p.uint256),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    pause: fun("0x8456cb59", "pause()", {}, ),
    paused: viewFun("0x5c975abb", "paused()", {}, p.bool),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    royaltiesManager: viewFun("0x967b0049", "royaltiesManager()", {}, p.address),
    royaltiesRate: viewFun("0xe74769c2", "royaltiesRate()", {}, p.uint256),
    signatureUses: viewFun("0x95d78012", "signatureUses(bytes32)", {"_0": p.bytes32}, p.uint256),
    signerSignatureIndex: viewFun("0xadc3fdf3", "signerSignatureIndex(address)", {"_0": p.address}, p.uint256),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
    unpause: fun("0x3f4ba83a", "unpause()", {}, ),
    updateCouponManager: fun("0x94188e93", "updateCouponManager(address)", {"_couponManager": p.address}, ),
    updateFeeCollector: fun("0xd2c35ce8", "updateFeeCollector(address)", {"_feeCollector": p.address}, ),
    updateFeeRate: fun("0x7b84fda5", "updateFeeRate(uint256)", {"_feeRate": p.uint256}, ),
    updateManaUsdAggregator: fun("0xa9d118d3", "updateManaUsdAggregator(address,uint256)", {"_aggregator": p.address, "_tolerance": p.uint256}, ),
    updateRoyaltiesManager: fun("0x6187bb4f", "updateRoyaltiesManager(address)", {"_royaltiesManager": p.address}, ),
    updateRoyaltiesRate: fun("0x87b33a21", "updateRoyaltiesRate(uint256)", {"_royaltiesRate": p.uint256}, ),
    usedTradeIds: viewFun("0xba3ecc2f", "usedTradeIds(bytes32)", {"_0": p.bytes32}, p.bool),
}

export class Contract extends ContractBase {

    ASSET_TYPE_COLLECTION_ITEM() {
        return this.eth_call(functions.ASSET_TYPE_COLLECTION_ITEM, {})
    }

    ASSET_TYPE_ERC20() {
        return this.eth_call(functions.ASSET_TYPE_ERC20, {})
    }

    ASSET_TYPE_ERC721() {
        return this.eth_call(functions.ASSET_TYPE_ERC721, {})
    }

    ASSET_TYPE_USD_PEGGED_MANA() {
        return this.eth_call(functions.ASSET_TYPE_USD_PEGGED_MANA, {})
    }

    cancelledSignatures(_0: CancelledSignaturesParams["_0"]) {
        return this.eth_call(functions.cancelledSignatures, {_0})
    }

    contractSignatureIndex() {
        return this.eth_call(functions.contractSignatureIndex, {})
    }

    couponManager() {
        return this.eth_call(functions.couponManager, {})
    }

    feeCollector() {
        return this.eth_call(functions.feeCollector, {})
    }

    feeRate() {
        return this.eth_call(functions.feeRate, {})
    }

    getNonce(_signer: GetNonceParams["_signer"]) {
        return this.eth_call(functions.getNonce, {_signer})
    }

    getTradeId(_trade: GetTradeIdParams["_trade"], _caller: GetTradeIdParams["_caller"]) {
        return this.eth_call(functions.getTradeId, {_trade, _caller})
    }

    manaAddress() {
        return this.eth_call(functions.manaAddress, {})
    }

    manaUsdAggregator() {
        return this.eth_call(functions.manaUsdAggregator, {})
    }

    manaUsdAggregatorTolerance() {
        return this.eth_call(functions.manaUsdAggregatorTolerance, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    paused() {
        return this.eth_call(functions.paused, {})
    }

    royaltiesManager() {
        return this.eth_call(functions.royaltiesManager, {})
    }

    royaltiesRate() {
        return this.eth_call(functions.royaltiesRate, {})
    }

    signatureUses(_0: SignatureUsesParams["_0"]) {
        return this.eth_call(functions.signatureUses, {_0})
    }

    signerSignatureIndex(_0: SignerSignatureIndexParams["_0"]) {
        return this.eth_call(functions.signerSignatureIndex, {_0})
    }

    usedTradeIds(_0: UsedTradeIdsParams["_0"]) {
        return this.eth_call(functions.usedTradeIds, {_0})
    }
}

/// Event types
export type ContractSignatureIndexIncreasedEventArgs = EParams<typeof events.ContractSignatureIndexIncreased>
export type CouponManagerUpdatedEventArgs = EParams<typeof events.CouponManagerUpdated>
export type FeeCollectorUpdatedEventArgs = EParams<typeof events.FeeCollectorUpdated>
export type FeeRateUpdatedEventArgs = EParams<typeof events.FeeRateUpdated>
export type ManaUsdAggregatorUpdatedEventArgs = EParams<typeof events.ManaUsdAggregatorUpdated>
export type MetaTransactionExecutedEventArgs = EParams<typeof events.MetaTransactionExecuted>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type PausedEventArgs = EParams<typeof events.Paused>
export type RoyaltiesManagerUpdatedEventArgs = EParams<typeof events.RoyaltiesManagerUpdated>
export type RoyaltiesRateUpdatedEventArgs = EParams<typeof events.RoyaltiesRateUpdated>
export type SignatureCancelledEventArgs = EParams<typeof events.SignatureCancelled>
export type SignerSignatureIndexIncreasedEventArgs = EParams<typeof events.SignerSignatureIndexIncreased>
export type TradedEventArgs = EParams<typeof events.Traded>
export type UnpausedEventArgs = EParams<typeof events.Unpaused>

/// Function types
export type ASSET_TYPE_COLLECTION_ITEMParams = FunctionArguments<typeof functions.ASSET_TYPE_COLLECTION_ITEM>
export type ASSET_TYPE_COLLECTION_ITEMReturn = FunctionReturn<typeof functions.ASSET_TYPE_COLLECTION_ITEM>

export type ASSET_TYPE_ERC20Params = FunctionArguments<typeof functions.ASSET_TYPE_ERC20>
export type ASSET_TYPE_ERC20Return = FunctionReturn<typeof functions.ASSET_TYPE_ERC20>

export type ASSET_TYPE_ERC721Params = FunctionArguments<typeof functions.ASSET_TYPE_ERC721>
export type ASSET_TYPE_ERC721Return = FunctionReturn<typeof functions.ASSET_TYPE_ERC721>

export type ASSET_TYPE_USD_PEGGED_MANAParams = FunctionArguments<typeof functions.ASSET_TYPE_USD_PEGGED_MANA>
export type ASSET_TYPE_USD_PEGGED_MANAReturn = FunctionReturn<typeof functions.ASSET_TYPE_USD_PEGGED_MANA>

export type AcceptParams = FunctionArguments<typeof functions.accept>
export type AcceptReturn = FunctionReturn<typeof functions.accept>

export type AcceptWithCouponParams = FunctionArguments<typeof functions.acceptWithCoupon>
export type AcceptWithCouponReturn = FunctionReturn<typeof functions.acceptWithCoupon>

export type CancelSignatureParams = FunctionArguments<typeof functions.cancelSignature>
export type CancelSignatureReturn = FunctionReturn<typeof functions.cancelSignature>

export type CancelledSignaturesParams = FunctionArguments<typeof functions.cancelledSignatures>
export type CancelledSignaturesReturn = FunctionReturn<typeof functions.cancelledSignatures>

export type ContractSignatureIndexParams = FunctionArguments<typeof functions.contractSignatureIndex>
export type ContractSignatureIndexReturn = FunctionReturn<typeof functions.contractSignatureIndex>

export type CouponManagerParams = FunctionArguments<typeof functions.couponManager>
export type CouponManagerReturn = FunctionReturn<typeof functions.couponManager>

export type ExecuteMetaTransactionParams = FunctionArguments<typeof functions.executeMetaTransaction>
export type ExecuteMetaTransactionReturn = FunctionReturn<typeof functions.executeMetaTransaction>

export type FeeCollectorParams = FunctionArguments<typeof functions.feeCollector>
export type FeeCollectorReturn = FunctionReturn<typeof functions.feeCollector>

export type FeeRateParams = FunctionArguments<typeof functions.feeRate>
export type FeeRateReturn = FunctionReturn<typeof functions.feeRate>

export type GetNonceParams = FunctionArguments<typeof functions.getNonce>
export type GetNonceReturn = FunctionReturn<typeof functions.getNonce>

export type GetTradeIdParams = FunctionArguments<typeof functions.getTradeId>
export type GetTradeIdReturn = FunctionReturn<typeof functions.getTradeId>

export type IncreaseContractSignatureIndexParams = FunctionArguments<typeof functions.increaseContractSignatureIndex>
export type IncreaseContractSignatureIndexReturn = FunctionReturn<typeof functions.increaseContractSignatureIndex>

export type IncreaseSignerSignatureIndexParams = FunctionArguments<typeof functions.increaseSignerSignatureIndex>
export type IncreaseSignerSignatureIndexReturn = FunctionReturn<typeof functions.increaseSignerSignatureIndex>

export type ManaAddressParams = FunctionArguments<typeof functions.manaAddress>
export type ManaAddressReturn = FunctionReturn<typeof functions.manaAddress>

export type ManaUsdAggregatorParams = FunctionArguments<typeof functions.manaUsdAggregator>
export type ManaUsdAggregatorReturn = FunctionReturn<typeof functions.manaUsdAggregator>

export type ManaUsdAggregatorToleranceParams = FunctionArguments<typeof functions.manaUsdAggregatorTolerance>
export type ManaUsdAggregatorToleranceReturn = FunctionReturn<typeof functions.manaUsdAggregatorTolerance>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type PauseParams = FunctionArguments<typeof functions.pause>
export type PauseReturn = FunctionReturn<typeof functions.pause>

export type PausedParams = FunctionArguments<typeof functions.paused>
export type PausedReturn = FunctionReturn<typeof functions.paused>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type RoyaltiesManagerParams = FunctionArguments<typeof functions.royaltiesManager>
export type RoyaltiesManagerReturn = FunctionReturn<typeof functions.royaltiesManager>

export type RoyaltiesRateParams = FunctionArguments<typeof functions.royaltiesRate>
export type RoyaltiesRateReturn = FunctionReturn<typeof functions.royaltiesRate>

export type SignatureUsesParams = FunctionArguments<typeof functions.signatureUses>
export type SignatureUsesReturn = FunctionReturn<typeof functions.signatureUses>

export type SignerSignatureIndexParams = FunctionArguments<typeof functions.signerSignatureIndex>
export type SignerSignatureIndexReturn = FunctionReturn<typeof functions.signerSignatureIndex>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type UnpauseParams = FunctionArguments<typeof functions.unpause>
export type UnpauseReturn = FunctionReturn<typeof functions.unpause>

export type UpdateCouponManagerParams = FunctionArguments<typeof functions.updateCouponManager>
export type UpdateCouponManagerReturn = FunctionReturn<typeof functions.updateCouponManager>

export type UpdateFeeCollectorParams = FunctionArguments<typeof functions.updateFeeCollector>
export type UpdateFeeCollectorReturn = FunctionReturn<typeof functions.updateFeeCollector>

export type UpdateFeeRateParams = FunctionArguments<typeof functions.updateFeeRate>
export type UpdateFeeRateReturn = FunctionReturn<typeof functions.updateFeeRate>

export type UpdateManaUsdAggregatorParams = FunctionArguments<typeof functions.updateManaUsdAggregator>
export type UpdateManaUsdAggregatorReturn = FunctionReturn<typeof functions.updateManaUsdAggregator>

export type UpdateRoyaltiesManagerParams = FunctionArguments<typeof functions.updateRoyaltiesManager>
export type UpdateRoyaltiesManagerReturn = FunctionReturn<typeof functions.updateRoyaltiesManager>

export type UpdateRoyaltiesRateParams = FunctionArguments<typeof functions.updateRoyaltiesRate>
export type UpdateRoyaltiesRateReturn = FunctionReturn<typeof functions.updateRoyaltiesRate>

export type UsedTradeIdsParams = FunctionArguments<typeof functions.usedTradeIds>
export type UsedTradeIdsReturn = FunctionReturn<typeof functions.usedTradeIds>

