import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    Transfer: event("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)", {"_from": indexed(p.address), "_to": indexed(p.address), "_tokenId": indexed(p.uint256)}),
    Approval: event("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)", {"_owner": indexed(p.address), "_approved": indexed(p.address), "_tokenId": indexed(p.uint256)}),
    ApprovalForAll: event("0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31", "ApprovalForAll(address,address,bool)", {"_owner": indexed(p.address), "_operator": indexed(p.address), "_approved": p.bool}),
    CreateEstate: event("0xd66691e9db811aef0bc0900328bd314b23f1f2285d5cb6d4baa4d959b3645a3c", "CreateEstate(address,uint256,string)", {"_owner": indexed(p.address), "_estateId": indexed(p.uint256), "_data": p.string}),
    AddLand: event("0xff0e52667d53255667dc777a00af81038a4646367b0d73d8ee8540ca5b0c9a2e", "AddLand(uint256,uint256)", {"_estateId": indexed(p.uint256), "_landId": indexed(p.uint256)}),
    RemoveLand: event("0x7932eb5ab0d4d4d172776074ee15d13d708465ff5476902ed15a4965434fcab1", "RemoveLand(uint256,uint256,address)", {"_estateId": indexed(p.uint256), "_landId": indexed(p.uint256), "_destinatary": indexed(p.address)}),
    Update: event("0x47c705b9219229ad762fca605f08fb024a3415d0ae78af5d319820c72e510414", "Update(uint256,address,address,string)", {"_assetId": indexed(p.uint256), "_holder": indexed(p.address), "_operator": indexed(p.address), "_data": p.string}),
    UpdateOperator: event("0x9d9dd80a56a16f715df6eb40b771e24ff8cbea6eed9de28473ce0f28fe5602a9", "UpdateOperator(uint256,address)", {"_estateId": indexed(p.uint256), "_operator": indexed(p.address)}),
    UpdateManager: event("0xd79fbfe1644c022b9150727d871532bfcc3e27ffee86fc596a062770ac97b042", "UpdateManager(address,address,address,bool)", {"_owner": indexed(p.address), "_operator": indexed(p.address), "_caller": indexed(p.address), "_approved": p.bool}),
    SetLANDRegistry: event("0x2e88792af6f2248ed486ffffc86edf9bc197596990f7b406d5f867a1dd930ba5", "SetLANDRegistry(address)", {"_registry": indexed(p.address)}),
    Migrated: event("0xdd117a11c22118c9dee4b5a67ce578bc44529dce21ee0ccc439588fbb9fb4ea3", "Migrated(string,string)", {"contractName": p.string, "migrationId": p.string}),
}

export const functions = {
    supportsInterface: viewFun("0x01ffc9a7", "supportsInterface(bytes4)", {"_interfaceId": p.bytes4}, p.bool),
    name: viewFun("0x06fdde03", "name()", {}, p.string),
    updateManager: viewFun("0x07ecec3e", "updateManager(address,address)", {"_0": p.address, "_1": p.address}, p.bool),
    getApproved: viewFun("0x081812fc", "getApproved(uint256)", {"_tokenId": p.uint256}, p.address),
    approve: fun("0x095ea7b3", "approve(address,uint256)", {"_to": p.address, "_tokenId": p.uint256}, ),
    landIdEstate: viewFun("0x0a354fce", "landIdEstate(uint256)", {"_0": p.uint256}, p.uint256),
    totalSupply: viewFun("0x18160ddd", "totalSupply()", {}, p.uint256),
    tokenOfOwnerByIndex: viewFun("0x2f745c59", "tokenOfOwnerByIndex(address,uint256)", {"_owner": p.address, "_index": p.uint256}, p.uint256),
    estateLandIds: viewFun("0x3970bfd3", "estateLandIds(uint256,uint256)", {"_0": p.uint256, "_1": p.uint256}, p.uint256),
    'safeTransferFrom(address,address,uint256)': fun("0x42842e0e", "safeTransferFrom(address,address,uint256)", {"_from": p.address, "_to": p.address, "_tokenId": p.uint256}, ),
    exists: viewFun("0x4f558e79", "exists(uint256)", {"_tokenId": p.uint256}, p.bool),
    tokenByIndex: viewFun("0x4f6ccce7", "tokenByIndex(uint256)", {"_index": p.uint256}, p.uint256),
    ownerOf: viewFun("0x6352211e", "ownerOf(uint256)", {"_tokenId": p.uint256}, p.address),
    balanceOf: viewFun("0x70a08231", "balanceOf(address)", {"_owner": p.address}, p.uint256),
    registry: viewFun("0x7b103999", "registry()", {}, p.address),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    symbol: viewFun("0x95d89b41", "symbol()", {}, p.string),
    updateOperator: viewFun("0x9d40b850", "updateOperator(uint256)", {"_0": p.uint256}, p.address),
    estateLandIndex: viewFun("0x9f813b1b", "estateLandIndex(uint256,uint256)", {"_0": p.uint256, "_1": p.uint256}, p.uint256),
    setApprovalForAll: fun("0xa22cb465", "setApprovalForAll(address,bool)", {"_to": p.address, "_approved": p.bool}, ),
    'safeTransferFrom(address,address,uint256,bytes)': fun("0xb88d4fde", "safeTransferFrom(address,address,uint256,bytes)", {"_from": p.address, "_to": p.address, "_tokenId": p.uint256, "_data": p.bytes}, ),
    isMigrated: viewFun("0xc0bac1a8", "isMigrated(string,string)", {"contractName": p.string, "migrationId": p.string}, p.bool),
    tokenURI: viewFun("0xc87b56dd", "tokenURI(uint256)", {"_tokenId": p.uint256}, p.string),
    isApprovedForAll: viewFun("0xe985e9c5", "isApprovedForAll(address,address)", {"_owner": p.address, "_operator": p.address}, p.bool),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
    mint: fun("0xd0def521", "mint(address,string)", {"to": p.address, "metadata": p.string}, p.uint256),
    transferLand: fun("0xa506e5dc", "transferLand(uint256,uint256,address)", {"estateId": p.uint256, "landId": p.uint256, "destinatary": p.address}, ),
    transferManyLands: fun("0x40807049", "transferManyLands(uint256,uint256[],address)", {"estateId": p.uint256, "landIds": p.array(p.uint256), "destinatary": p.address}, ),
    getLandEstateId: viewFun("0xbb969132", "getLandEstateId(uint256)", {"landId": p.uint256}, p.uint256),
    setLANDRegistry: fun("0x535a920c", "setLANDRegistry(address)", {"_registry": p.address}, ),
    ping: fun("0x5c36b186", "ping()", {}, ),
    getEstateSize: viewFun("0xf4a43448", "getEstateSize(uint256)", {"estateId": p.uint256}, p.uint256),
    updateMetadata: fun("0x53c8388e", "updateMetadata(uint256,string)", {"estateId": p.uint256, "metadata": p.string}, ),
    getMetadata: viewFun("0xa574cea4", "getMetadata(uint256)", {"estateId": p.uint256}, p.string),
    isUpdateAuthorized: viewFun("0x65937ab9", "isUpdateAuthorized(address,uint256)", {"operator": p.address, "estateId": p.uint256}, p.bool),
    setUpdateManager: fun("0xef1db762", "setUpdateManager(address,address,bool)", {"_owner": p.address, "_operator": p.address, "_approved": p.bool}, ),
    setUpdateOperator: fun("0xb0b02c60", "setUpdateOperator(uint256,address)", {"estateId": p.uint256, "operator": p.address}, ),
    setLandUpdateOperator: fun("0x1d11016a", "setLandUpdateOperator(uint256,uint256,address)", {"estateId": p.uint256, "landId": p.uint256, "operator": p.address}, ),
    'initialize(string,string)': fun("0x4cd88b76", "initialize(string,string)", {"_name": p.string, "_symbol": p.string}, ),
    'initialize()': fun("0x8129fc1c", "initialize()", {}, ),
    'initialize(string,string,address)': fun("0x077f224a", "initialize(string,string,address)", {"_name": p.string, "_symbol": p.string, "_registry": p.address}, ),
    'initialize(address)': fun("0xc4d66de8", "initialize(address)", {"_sender": p.address}, ),
    onERC721Received: fun("0x150b7a02", "onERC721Received(address,address,uint256,bytes)", {"_operator": p.address, "_from": p.address, "_tokenId": p.uint256, "_data": p.bytes}, p.bytes4),
    getFingerprint: viewFun("0x159a6475", "getFingerprint(uint256)", {"estateId": p.uint256}, p.bytes32),
    verifyFingerprint: viewFun("0x8f9f4b63", "verifyFingerprint(uint256,bytes)", {"estateId": p.uint256, "fingerprint": p.bytes}, p.bool),
    'safeTransferManyFrom(address,address,uint256[])': fun("0x73b913fa", "safeTransferManyFrom(address,address,uint256[])", {"from": p.address, "to": p.address, "estateIds": p.array(p.uint256)}, ),
    'safeTransferManyFrom(address,address,uint256[],bytes)': fun("0xd93eeb5c", "safeTransferManyFrom(address,address,uint256[],bytes)", {"from": p.address, "to": p.address, "estateIds": p.array(p.uint256), "data": p.bytes}, ),
    updateLandData: fun("0x28b34ef6", "updateLandData(uint256,uint256,string)", {"estateId": p.uint256, "landId": p.uint256, "data": p.string}, ),
    updateManyLandData: fun("0x426a0af3", "updateManyLandData(uint256,uint256[],string)", {"estateId": p.uint256, "landIds": p.array(p.uint256), "data": p.string}, ),
    transferFrom: fun("0x23b872dd", "transferFrom(address,address,uint256)", {"_from": p.address, "_to": p.address, "_tokenId": p.uint256}, ),
}

export class Contract extends ContractBase {

    supportsInterface(_interfaceId: SupportsInterfaceParams["_interfaceId"]) {
        return this.eth_call(functions.supportsInterface, {_interfaceId})
    }

    name() {
        return this.eth_call(functions.name, {})
    }

    updateManager(_0: UpdateManagerParams["_0"], _1: UpdateManagerParams["_1"]) {
        return this.eth_call(functions.updateManager, {_0, _1})
    }

    getApproved(_tokenId: GetApprovedParams["_tokenId"]) {
        return this.eth_call(functions.getApproved, {_tokenId})
    }

    landIdEstate(_0: LandIdEstateParams["_0"]) {
        return this.eth_call(functions.landIdEstate, {_0})
    }

    totalSupply() {
        return this.eth_call(functions.totalSupply, {})
    }

    tokenOfOwnerByIndex(_owner: TokenOfOwnerByIndexParams["_owner"], _index: TokenOfOwnerByIndexParams["_index"]) {
        return this.eth_call(functions.tokenOfOwnerByIndex, {_owner, _index})
    }

    estateLandIds(_0: EstateLandIdsParams["_0"], _1: EstateLandIdsParams["_1"]) {
        return this.eth_call(functions.estateLandIds, {_0, _1})
    }

    exists(_tokenId: ExistsParams["_tokenId"]) {
        return this.eth_call(functions.exists, {_tokenId})
    }

    tokenByIndex(_index: TokenByIndexParams["_index"]) {
        return this.eth_call(functions.tokenByIndex, {_index})
    }

    ownerOf(_tokenId: OwnerOfParams["_tokenId"]) {
        return this.eth_call(functions.ownerOf, {_tokenId})
    }

    balanceOf(_owner: BalanceOfParams["_owner"]) {
        return this.eth_call(functions.balanceOf, {_owner})
    }

    registry() {
        return this.eth_call(functions.registry, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    symbol() {
        return this.eth_call(functions.symbol, {})
    }

    updateOperator(_0: UpdateOperatorParams["_0"]) {
        return this.eth_call(functions.updateOperator, {_0})
    }

    estateLandIndex(_0: EstateLandIndexParams["_0"], _1: EstateLandIndexParams["_1"]) {
        return this.eth_call(functions.estateLandIndex, {_0, _1})
    }

    isMigrated(contractName: IsMigratedParams["contractName"], migrationId: IsMigratedParams["migrationId"]) {
        return this.eth_call(functions.isMigrated, {contractName, migrationId})
    }

    tokenURI(_tokenId: TokenURIParams["_tokenId"]) {
        return this.eth_call(functions.tokenURI, {_tokenId})
    }

    isApprovedForAll(_owner: IsApprovedForAllParams["_owner"], _operator: IsApprovedForAllParams["_operator"]) {
        return this.eth_call(functions.isApprovedForAll, {_owner, _operator})
    }

    getLandEstateId(landId: GetLandEstateIdParams["landId"]) {
        return this.eth_call(functions.getLandEstateId, {landId})
    }

    getEstateSize(estateId: GetEstateSizeParams["estateId"]) {
        return this.eth_call(functions.getEstateSize, {estateId})
    }

    getMetadata(estateId: GetMetadataParams["estateId"]) {
        return this.eth_call(functions.getMetadata, {estateId})
    }

    isUpdateAuthorized(operator: IsUpdateAuthorizedParams["operator"], estateId: IsUpdateAuthorizedParams["estateId"]) {
        return this.eth_call(functions.isUpdateAuthorized, {operator, estateId})
    }

    getFingerprint(estateId: GetFingerprintParams["estateId"]) {
        return this.eth_call(functions.getFingerprint, {estateId})
    }

    verifyFingerprint(estateId: VerifyFingerprintParams["estateId"], fingerprint: VerifyFingerprintParams["fingerprint"]) {
        return this.eth_call(functions.verifyFingerprint, {estateId, fingerprint})
    }
}

/// Event types
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type TransferEventArgs = EParams<typeof events.Transfer>
export type ApprovalEventArgs = EParams<typeof events.Approval>
export type ApprovalForAllEventArgs = EParams<typeof events.ApprovalForAll>
export type CreateEstateEventArgs = EParams<typeof events.CreateEstate>
export type AddLandEventArgs = EParams<typeof events.AddLand>
export type RemoveLandEventArgs = EParams<typeof events.RemoveLand>
export type UpdateEventArgs = EParams<typeof events.Update>
export type UpdateOperatorEventArgs = EParams<typeof events.UpdateOperator>
export type UpdateManagerEventArgs = EParams<typeof events.UpdateManager>
export type SetLANDRegistryEventArgs = EParams<typeof events.SetLANDRegistry>
export type MigratedEventArgs = EParams<typeof events.Migrated>

/// Function types
export type SupportsInterfaceParams = FunctionArguments<typeof functions.supportsInterface>
export type SupportsInterfaceReturn = FunctionReturn<typeof functions.supportsInterface>

export type NameParams = FunctionArguments<typeof functions.name>
export type NameReturn = FunctionReturn<typeof functions.name>

export type UpdateManagerParams = FunctionArguments<typeof functions.updateManager>
export type UpdateManagerReturn = FunctionReturn<typeof functions.updateManager>

export type GetApprovedParams = FunctionArguments<typeof functions.getApproved>
export type GetApprovedReturn = FunctionReturn<typeof functions.getApproved>

export type ApproveParams = FunctionArguments<typeof functions.approve>
export type ApproveReturn = FunctionReturn<typeof functions.approve>

export type LandIdEstateParams = FunctionArguments<typeof functions.landIdEstate>
export type LandIdEstateReturn = FunctionReturn<typeof functions.landIdEstate>

export type TotalSupplyParams = FunctionArguments<typeof functions.totalSupply>
export type TotalSupplyReturn = FunctionReturn<typeof functions.totalSupply>

export type TokenOfOwnerByIndexParams = FunctionArguments<typeof functions.tokenOfOwnerByIndex>
export type TokenOfOwnerByIndexReturn = FunctionReturn<typeof functions.tokenOfOwnerByIndex>

export type EstateLandIdsParams = FunctionArguments<typeof functions.estateLandIds>
export type EstateLandIdsReturn = FunctionReturn<typeof functions.estateLandIds>

export type SafeTransferFromParams_0 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256)']>
export type SafeTransferFromReturn_0 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256)']>

export type ExistsParams = FunctionArguments<typeof functions.exists>
export type ExistsReturn = FunctionReturn<typeof functions.exists>

export type TokenByIndexParams = FunctionArguments<typeof functions.tokenByIndex>
export type TokenByIndexReturn = FunctionReturn<typeof functions.tokenByIndex>

export type OwnerOfParams = FunctionArguments<typeof functions.ownerOf>
export type OwnerOfReturn = FunctionReturn<typeof functions.ownerOf>

export type BalanceOfParams = FunctionArguments<typeof functions.balanceOf>
export type BalanceOfReturn = FunctionReturn<typeof functions.balanceOf>

export type RegistryParams = FunctionArguments<typeof functions.registry>
export type RegistryReturn = FunctionReturn<typeof functions.registry>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type SymbolParams = FunctionArguments<typeof functions.symbol>
export type SymbolReturn = FunctionReturn<typeof functions.symbol>

export type UpdateOperatorParams = FunctionArguments<typeof functions.updateOperator>
export type UpdateOperatorReturn = FunctionReturn<typeof functions.updateOperator>

export type EstateLandIndexParams = FunctionArguments<typeof functions.estateLandIndex>
export type EstateLandIndexReturn = FunctionReturn<typeof functions.estateLandIndex>

export type SetApprovalForAllParams = FunctionArguments<typeof functions.setApprovalForAll>
export type SetApprovalForAllReturn = FunctionReturn<typeof functions.setApprovalForAll>

export type SafeTransferFromParams_1 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>
export type SafeTransferFromReturn_1 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>

export type IsMigratedParams = FunctionArguments<typeof functions.isMigrated>
export type IsMigratedReturn = FunctionReturn<typeof functions.isMigrated>

export type TokenURIParams = FunctionArguments<typeof functions.tokenURI>
export type TokenURIReturn = FunctionReturn<typeof functions.tokenURI>

export type IsApprovedForAllParams = FunctionArguments<typeof functions.isApprovedForAll>
export type IsApprovedForAllReturn = FunctionReturn<typeof functions.isApprovedForAll>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type MintParams = FunctionArguments<typeof functions.mint>
export type MintReturn = FunctionReturn<typeof functions.mint>

export type TransferLandParams = FunctionArguments<typeof functions.transferLand>
export type TransferLandReturn = FunctionReturn<typeof functions.transferLand>

export type TransferManyLandsParams = FunctionArguments<typeof functions.transferManyLands>
export type TransferManyLandsReturn = FunctionReturn<typeof functions.transferManyLands>

export type GetLandEstateIdParams = FunctionArguments<typeof functions.getLandEstateId>
export type GetLandEstateIdReturn = FunctionReturn<typeof functions.getLandEstateId>

export type SetLANDRegistryParams = FunctionArguments<typeof functions.setLANDRegistry>
export type SetLANDRegistryReturn = FunctionReturn<typeof functions.setLANDRegistry>

export type PingParams = FunctionArguments<typeof functions.ping>
export type PingReturn = FunctionReturn<typeof functions.ping>

export type GetEstateSizeParams = FunctionArguments<typeof functions.getEstateSize>
export type GetEstateSizeReturn = FunctionReturn<typeof functions.getEstateSize>

export type UpdateMetadataParams = FunctionArguments<typeof functions.updateMetadata>
export type UpdateMetadataReturn = FunctionReturn<typeof functions.updateMetadata>

export type GetMetadataParams = FunctionArguments<typeof functions.getMetadata>
export type GetMetadataReturn = FunctionReturn<typeof functions.getMetadata>

export type IsUpdateAuthorizedParams = FunctionArguments<typeof functions.isUpdateAuthorized>
export type IsUpdateAuthorizedReturn = FunctionReturn<typeof functions.isUpdateAuthorized>

export type SetUpdateManagerParams = FunctionArguments<typeof functions.setUpdateManager>
export type SetUpdateManagerReturn = FunctionReturn<typeof functions.setUpdateManager>

export type SetUpdateOperatorParams = FunctionArguments<typeof functions.setUpdateOperator>
export type SetUpdateOperatorReturn = FunctionReturn<typeof functions.setUpdateOperator>

export type SetLandUpdateOperatorParams = FunctionArguments<typeof functions.setLandUpdateOperator>
export type SetLandUpdateOperatorReturn = FunctionReturn<typeof functions.setLandUpdateOperator>

export type InitializeParams_0 = FunctionArguments<typeof functions['initialize(string,string)']>
export type InitializeReturn_0 = FunctionReturn<typeof functions['initialize(string,string)']>

export type InitializeParams_1 = FunctionArguments<typeof functions['initialize()']>
export type InitializeReturn_1 = FunctionReturn<typeof functions['initialize()']>

export type InitializeParams_2 = FunctionArguments<typeof functions['initialize(string,string,address)']>
export type InitializeReturn_2 = FunctionReturn<typeof functions['initialize(string,string,address)']>

export type InitializeParams_3 = FunctionArguments<typeof functions['initialize(address)']>
export type InitializeReturn_3 = FunctionReturn<typeof functions['initialize(address)']>

export type OnERC721ReceivedParams = FunctionArguments<typeof functions.onERC721Received>
export type OnERC721ReceivedReturn = FunctionReturn<typeof functions.onERC721Received>

export type GetFingerprintParams = FunctionArguments<typeof functions.getFingerprint>
export type GetFingerprintReturn = FunctionReturn<typeof functions.getFingerprint>

export type VerifyFingerprintParams = FunctionArguments<typeof functions.verifyFingerprint>
export type VerifyFingerprintReturn = FunctionReturn<typeof functions.verifyFingerprint>

export type SafeTransferManyFromParams_0 = FunctionArguments<typeof functions['safeTransferManyFrom(address,address,uint256[])']>
export type SafeTransferManyFromReturn_0 = FunctionReturn<typeof functions['safeTransferManyFrom(address,address,uint256[])']>

export type SafeTransferManyFromParams_1 = FunctionArguments<typeof functions['safeTransferManyFrom(address,address,uint256[],bytes)']>
export type SafeTransferManyFromReturn_1 = FunctionReturn<typeof functions['safeTransferManyFrom(address,address,uint256[],bytes)']>

export type UpdateLandDataParams = FunctionArguments<typeof functions.updateLandData>
export type UpdateLandDataReturn = FunctionReturn<typeof functions.updateLandData>

export type UpdateManyLandDataParams = FunctionArguments<typeof functions.updateManyLandData>
export type UpdateManyLandDataReturn = FunctionReturn<typeof functions.updateManyLandData>

export type TransferFromParams = FunctionArguments<typeof functions.transferFrom>
export type TransferFromReturn = FunctionReturn<typeof functions.transferFrom>

