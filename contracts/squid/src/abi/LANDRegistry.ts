import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    EstateRegistrySet: event("0x5b3ad80e09ce4c88735037542891c02fdc63b85e96a579d9260a403153c27755", "EstateRegistrySet(address)", {"registry": indexed(p.address)}),
    Update: event("0x47c705b9219229ad762fca605f08fb024a3415d0ae78af5d319820c72e510414", "Update(uint256,address,address,string)", {"assetId": indexed(p.uint256), "holder": indexed(p.address), "operator": indexed(p.address), "data": p.string}),
    UpdateOperator: event("0x9d9dd80a56a16f715df6eb40b771e24ff8cbea6eed9de28473ce0f28fe5602a9", "UpdateOperator(uint256,address)", {"assetId": indexed(p.uint256), "operator": indexed(p.address)}),
    UpdateManager: event("0xd79fbfe1644c022b9150727d871532bfcc3e27ffee86fc596a062770ac97b042", "UpdateManager(address,address,address,bool)", {"_owner": indexed(p.address), "_operator": indexed(p.address), "_caller": indexed(p.address), "_approved": p.bool}),
    DeployAuthorized: event("0x9e237638dcd1b2be1fc623ab6a47409b34dc8a62196448dd2d6b9045b2c33157", "DeployAuthorized(address,address)", {"_caller": indexed(p.address), "_deployer": indexed(p.address)}),
    DeployForbidden: event("0x7883da318e7694f396f400092b07b701a4eccb02aee0d28266adc659cca044da", "DeployForbidden(address,address)", {"_caller": indexed(p.address), "_deployer": indexed(p.address)}),
    'Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)': event("0x8988d59efc2c4547ef86c88f6543963bab0cea94f8e486e619c7c3a790db93be", "Transfer(address,address,uint256,address,bytes,bytes)", {"from": indexed(p.address), "to": indexed(p.address), "assetId": indexed(p.uint256), "operator": p.address, "userData": p.bytes, "operatorData": p.bytes}),
    'Transfer(address indexed,address indexed,uint256 indexed,address,bytes)': event("0xd5c97f2e041b2046be3b4337472f05720760a198f4d7d84980b7155eec7cca6f", "Transfer(address,address,uint256,address,bytes)", {"from": indexed(p.address), "to": indexed(p.address), "assetId": indexed(p.uint256), "operator": p.address, "userData": p.bytes}),
    'Transfer(address indexed,address indexed,uint256 indexed)': event("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)", {"from": indexed(p.address), "to": indexed(p.address), "assetId": indexed(p.uint256)}),
    ApprovalForAll: event("0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31", "ApprovalForAll(address,address,bool)", {"holder": indexed(p.address), "operator": indexed(p.address), "authorized": p.bool}),
    Approval: event("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)", {"owner": indexed(p.address), "operator": indexed(p.address), "assetId": indexed(p.uint256)}),
    OwnerUpdate: event("0x343765429aea5a34b3ff6a3785a98a5abb2597aca87bfbb58632c173d585373a", "OwnerUpdate(address,address)", {"_prevOwner": p.address, "_newOwner": p.address}),
}

export const functions = {
    supportsInterface: viewFun("0x01ffc9a7", "supportsInterface(bytes4)", {"_interfaceID": p.bytes4}, p.bool),
    proxyOwner: viewFun("0x025313a2", "proxyOwner()", {}, p.address),
    name: viewFun("0x06fdde03", "name()", {}, p.string),
    updateManager: viewFun("0x07ecec3e", "updateManager(address,address)", {"_0": p.address, "_1": p.address}, p.bool),
    getApproved: viewFun("0x081812fc", "getApproved(uint256)", {"assetId": p.uint256}, p.address),
    approve: fun("0x095ea7b3", "approve(address,uint256)", {"operator": p.address, "assetId": p.uint256}, ),
    totalSupply: viewFun("0x18160ddd", "totalSupply()", {}, p.uint256),
    latestPing: viewFun("0x1e375901", "latestPing(address)", {"_0": p.address}, p.uint256),
    isAuthorized: viewFun("0x2972b0f0", "isAuthorized(address,uint256)", {"operator": p.address, "assetId": p.uint256}, p.bool),
    authorizedDeploy: viewFun("0x29ffab3b", "authorizedDeploy(address)", {"_0": p.address}, p.bool),
    tokenOfOwnerByIndex: viewFun("0x2f745c59", "tokenOfOwnerByIndex(address,uint256)", {"owner": p.address, "index": p.uint256}, p.uint256),
    decimals: viewFun("0x313ce567", "decimals()", {}, p.uint256),
    'safeTransferFrom(address,address,uint256)': fun("0x42842e0e", "safeTransferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "assetId": p.uint256}, ),
    tokensOf: viewFun("0x5a3f2672", "tokensOf(address)", {"owner": p.address}, p.array(p.uint256)),
    ownerOf: viewFun("0x6352211e", "ownerOf(uint256)", {"assetId": p.uint256}, p.address),
    GET_METADATA: viewFun("0x65181ad3", "GET_METADATA()", {}, p.bytes4),
    balanceOf: viewFun("0x70a08231", "balanceOf(address)", {"owner": p.address}, p.uint256),
    currentContract: viewFun("0x721d7d8e", "currentContract()", {}, p.address),
    description: viewFun("0x7284e416", "description()", {}, p.string),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    symbol: viewFun("0x95d89b41", "symbol()", {}, p.string),
    updateOperator: viewFun("0x9d40b850", "updateOperator(uint256)", {"_0": p.uint256}, p.address),
    setApprovalForAll: fun("0xa22cb465", "setApprovalForAll(address,bool)", {"operator": p.address, "authorized": p.bool}, ),
    'safeTransferFrom(address,address,uint256,bytes)': fun("0xb88d4fde", "safeTransferFrom(address,address,uint256,bytes)", {"from": p.address, "to": p.address, "assetId": p.uint256, "userData": p.bytes}, ),
    estateRegistry: viewFun("0xe387d31a", "estateRegistry()", {}, p.address),
    isApprovedForAll: viewFun("0xe985e9c5", "isApprovedForAll(address,address)", {"assetHolder": p.address, "operator": p.address}, p.bool),
    getApprovedAddress: viewFun("0xeca4742a", "getApprovedAddress(uint256)", {"assetId": p.uint256}, p.address),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"_newOwner": p.address}, ),
    initialize: fun("0x439fab91", "initialize(bytes)", {"_0": p.bytes}, ),
    isUpdateAuthorized: viewFun("0x65937ab9", "isUpdateAuthorized(address,uint256)", {"operator": p.address, "assetId": p.uint256}, p.bool),
    authorizeDeploy: fun("0x341f13f2", "authorizeDeploy(address)", {"beneficiary": p.address}, ),
    forbidDeploy: fun("0xfd7a1b00", "forbidDeploy(address)", {"beneficiary": p.address}, ),
    assignNewParcel: fun("0x1cc69ac1", "assignNewParcel(int256,int256,address)", {"x": p.int256, "y": p.int256, "beneficiary": p.address}, ),
    assignMultipleParcels: fun("0x8668a416", "assignMultipleParcels(int256[],int256[],address)", {"x": p.array(p.int256), "y": p.array(p.int256), "beneficiary": p.address}, ),
    ping: fun("0x5c36b186", "ping()", {}, ),
    setLatestToNow: fun("0x133cbe3a", "setLatestToNow(address)", {"user": p.address}, ),
    encodeTokenId: viewFun("0x6fb7e588", "encodeTokenId(int256,int256)", {"x": p.int256, "y": p.int256}, p.uint256),
    decodeTokenId: viewFun("0x7efd9112", "decodeTokenId(uint256)", {"value": p.uint256}, {"_0": p.int256, "_1": p.int256}),
    'exists(int256,int256)': viewFun("0xa8d88127", "exists(int256,int256)", {"x": p.int256, "y": p.int256}, p.bool),
    'exists(uint256)': viewFun("0x4f558e79", "exists(uint256)", {"assetId": p.uint256}, p.bool),
    ownerOfLand: viewFun("0x1080f251", "ownerOfLand(int256,int256)", {"x": p.int256, "y": p.int256}, p.address),
    ownerOfLandMany: viewFun("0x1d4b11e4", "ownerOfLandMany(int256[],int256[])", {"x": p.array(p.int256), "y": p.array(p.int256)}, p.array(p.address)),
    landOf: viewFun("0x885363eb", "landOf(address)", {"owner": p.address}, {"_0": p.array(p.int256), "_1": p.array(p.int256)}),
    tokenMetadata: viewFun("0x6914db60", "tokenMetadata(uint256)", {"assetId": p.uint256}, p.string),
    landData: viewFun("0x4c4bf936", "landData(int256,int256)", {"x": p.int256, "y": p.int256}, p.string),
    transferFrom: fun("0x23b872dd", "transferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "assetId": p.uint256}, ),
    transferLand: fun("0x35e64aaa", "transferLand(int256,int256,address)", {"x": p.int256, "y": p.int256, "to": p.address}, ),
    transferManyLand: fun("0x4dad9003", "transferManyLand(int256[],int256[],address)", {"x": p.array(p.int256), "y": p.array(p.int256), "to": p.address}, ),
    transferLandToEstate: fun("0xf9cbec43", "transferLandToEstate(int256,int256,uint256)", {"x": p.int256, "y": p.int256, "estateId": p.uint256}, ),
    transferManyLandToEstate: fun("0xf5f83a61", "transferManyLandToEstate(int256[],int256[],uint256)", {"x": p.array(p.int256), "y": p.array(p.int256), "estateId": p.uint256}, ),
    setUpdateOperator: fun("0xb0b02c60", "setUpdateOperator(uint256,address)", {"assetId": p.uint256, "operator": p.address}, ),
    setUpdateManager: fun("0xef1db762", "setUpdateManager(address,address,bool)", {"_owner": p.address, "_operator": p.address, "_approved": p.bool}, ),
    setEstateRegistry: fun("0x8de74aa1", "setEstateRegistry(address)", {"registry": p.address}, ),
    createEstate: fun("0xca8a2c08", "createEstate(int256[],int256[],address)", {"x": p.array(p.int256), "y": p.array(p.int256), "beneficiary": p.address}, p.uint256),
    createEstateWithMetadata: fun("0x881eeaa5", "createEstateWithMetadata(int256[],int256[],address,string)", {"x": p.array(p.int256), "y": p.array(p.int256), "beneficiary": p.address, "metadata": p.string}, p.uint256),
    updateLandData: fun("0xd4dd1594", "updateLandData(int256,int256,string)", {"x": p.int256, "y": p.int256, "data": p.string}, ),
    updateManyLandData: fun("0x1e4c7736", "updateManyLandData(int256[],int256[],string)", {"x": p.array(p.int256), "y": p.array(p.int256), "data": p.string}, ),
}

export class Contract extends ContractBase {

    supportsInterface(_interfaceID: SupportsInterfaceParams["_interfaceID"]) {
        return this.eth_call(functions.supportsInterface, {_interfaceID})
    }

    proxyOwner() {
        return this.eth_call(functions.proxyOwner, {})
    }

    name() {
        return this.eth_call(functions.name, {})
    }

    updateManager(_0: UpdateManagerParams["_0"], _1: UpdateManagerParams["_1"]) {
        return this.eth_call(functions.updateManager, {_0, _1})
    }

    getApproved(assetId: GetApprovedParams["assetId"]) {
        return this.eth_call(functions.getApproved, {assetId})
    }

    totalSupply() {
        return this.eth_call(functions.totalSupply, {})
    }

    latestPing(_0: LatestPingParams["_0"]) {
        return this.eth_call(functions.latestPing, {_0})
    }

    isAuthorized(operator: IsAuthorizedParams["operator"], assetId: IsAuthorizedParams["assetId"]) {
        return this.eth_call(functions.isAuthorized, {operator, assetId})
    }

    authorizedDeploy(_0: AuthorizedDeployParams["_0"]) {
        return this.eth_call(functions.authorizedDeploy, {_0})
    }

    tokenOfOwnerByIndex(owner: TokenOfOwnerByIndexParams["owner"], index: TokenOfOwnerByIndexParams["index"]) {
        return this.eth_call(functions.tokenOfOwnerByIndex, {owner, index})
    }

    decimals() {
        return this.eth_call(functions.decimals, {})
    }

    tokensOf(owner: TokensOfParams["owner"]) {
        return this.eth_call(functions.tokensOf, {owner})
    }

    ownerOf(assetId: OwnerOfParams["assetId"]) {
        return this.eth_call(functions.ownerOf, {assetId})
    }

    GET_METADATA() {
        return this.eth_call(functions.GET_METADATA, {})
    }

    balanceOf(owner: BalanceOfParams["owner"]) {
        return this.eth_call(functions.balanceOf, {owner})
    }

    currentContract() {
        return this.eth_call(functions.currentContract, {})
    }

    description() {
        return this.eth_call(functions.description, {})
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

    estateRegistry() {
        return this.eth_call(functions.estateRegistry, {})
    }

    isApprovedForAll(assetHolder: IsApprovedForAllParams["assetHolder"], operator: IsApprovedForAllParams["operator"]) {
        return this.eth_call(functions.isApprovedForAll, {assetHolder, operator})
    }

    getApprovedAddress(assetId: GetApprovedAddressParams["assetId"]) {
        return this.eth_call(functions.getApprovedAddress, {assetId})
    }

    isUpdateAuthorized(operator: IsUpdateAuthorizedParams["operator"], assetId: IsUpdateAuthorizedParams["assetId"]) {
        return this.eth_call(functions.isUpdateAuthorized, {operator, assetId})
    }

    encodeTokenId(x: EncodeTokenIdParams["x"], y: EncodeTokenIdParams["y"]) {
        return this.eth_call(functions.encodeTokenId, {x, y})
    }

    decodeTokenId(value: DecodeTokenIdParams["value"]) {
        return this.eth_call(functions.decodeTokenId, {value})
    }

    'exists(int256,int256)'(x: ExistsParams_0["x"], y: ExistsParams_0["y"]) {
        return this.eth_call(functions['exists(int256,int256)'], {x, y})
    }

    'exists(uint256)'(assetId: ExistsParams_1["assetId"]) {
        return this.eth_call(functions['exists(uint256)'], {assetId})
    }

    ownerOfLand(x: OwnerOfLandParams["x"], y: OwnerOfLandParams["y"]) {
        return this.eth_call(functions.ownerOfLand, {x, y})
    }

    ownerOfLandMany(x: OwnerOfLandManyParams["x"], y: OwnerOfLandManyParams["y"]) {
        return this.eth_call(functions.ownerOfLandMany, {x, y})
    }

    landOf(owner: LandOfParams["owner"]) {
        return this.eth_call(functions.landOf, {owner})
    }

    tokenMetadata(assetId: TokenMetadataParams["assetId"]) {
        return this.eth_call(functions.tokenMetadata, {assetId})
    }

    landData(x: LandDataParams["x"], y: LandDataParams["y"]) {
        return this.eth_call(functions.landData, {x, y})
    }
}

/// Event types
export type EstateRegistrySetEventArgs = EParams<typeof events.EstateRegistrySet>
export type UpdateEventArgs = EParams<typeof events.Update>
export type UpdateOperatorEventArgs = EParams<typeof events.UpdateOperator>
export type UpdateManagerEventArgs = EParams<typeof events.UpdateManager>
export type DeployAuthorizedEventArgs = EParams<typeof events.DeployAuthorized>
export type DeployForbiddenEventArgs = EParams<typeof events.DeployForbidden>
export type TransferEventArgs_0 = EParams<typeof events['Transfer(address indexed,address indexed,uint256 indexed,address,bytes,bytes)']>
export type TransferEventArgs_1 = EParams<typeof events['Transfer(address indexed,address indexed,uint256 indexed,address,bytes)']>
export type TransferEventArgs_2 = EParams<typeof events['Transfer(address indexed,address indexed,uint256 indexed)']>
export type ApprovalForAllEventArgs = EParams<typeof events.ApprovalForAll>
export type ApprovalEventArgs = EParams<typeof events.Approval>
export type OwnerUpdateEventArgs = EParams<typeof events.OwnerUpdate>

/// Function types
export type SupportsInterfaceParams = FunctionArguments<typeof functions.supportsInterface>
export type SupportsInterfaceReturn = FunctionReturn<typeof functions.supportsInterface>

export type ProxyOwnerParams = FunctionArguments<typeof functions.proxyOwner>
export type ProxyOwnerReturn = FunctionReturn<typeof functions.proxyOwner>

export type NameParams = FunctionArguments<typeof functions.name>
export type NameReturn = FunctionReturn<typeof functions.name>

export type UpdateManagerParams = FunctionArguments<typeof functions.updateManager>
export type UpdateManagerReturn = FunctionReturn<typeof functions.updateManager>

export type GetApprovedParams = FunctionArguments<typeof functions.getApproved>
export type GetApprovedReturn = FunctionReturn<typeof functions.getApproved>

export type ApproveParams = FunctionArguments<typeof functions.approve>
export type ApproveReturn = FunctionReturn<typeof functions.approve>

export type TotalSupplyParams = FunctionArguments<typeof functions.totalSupply>
export type TotalSupplyReturn = FunctionReturn<typeof functions.totalSupply>

export type LatestPingParams = FunctionArguments<typeof functions.latestPing>
export type LatestPingReturn = FunctionReturn<typeof functions.latestPing>

export type IsAuthorizedParams = FunctionArguments<typeof functions.isAuthorized>
export type IsAuthorizedReturn = FunctionReturn<typeof functions.isAuthorized>

export type AuthorizedDeployParams = FunctionArguments<typeof functions.authorizedDeploy>
export type AuthorizedDeployReturn = FunctionReturn<typeof functions.authorizedDeploy>

export type TokenOfOwnerByIndexParams = FunctionArguments<typeof functions.tokenOfOwnerByIndex>
export type TokenOfOwnerByIndexReturn = FunctionReturn<typeof functions.tokenOfOwnerByIndex>

export type DecimalsParams = FunctionArguments<typeof functions.decimals>
export type DecimalsReturn = FunctionReturn<typeof functions.decimals>

export type SafeTransferFromParams_0 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256)']>
export type SafeTransferFromReturn_0 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256)']>

export type TokensOfParams = FunctionArguments<typeof functions.tokensOf>
export type TokensOfReturn = FunctionReturn<typeof functions.tokensOf>

export type OwnerOfParams = FunctionArguments<typeof functions.ownerOf>
export type OwnerOfReturn = FunctionReturn<typeof functions.ownerOf>

export type GET_METADATAParams = FunctionArguments<typeof functions.GET_METADATA>
export type GET_METADATAReturn = FunctionReturn<typeof functions.GET_METADATA>

export type BalanceOfParams = FunctionArguments<typeof functions.balanceOf>
export type BalanceOfReturn = FunctionReturn<typeof functions.balanceOf>

export type CurrentContractParams = FunctionArguments<typeof functions.currentContract>
export type CurrentContractReturn = FunctionReturn<typeof functions.currentContract>

export type DescriptionParams = FunctionArguments<typeof functions.description>
export type DescriptionReturn = FunctionReturn<typeof functions.description>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type SymbolParams = FunctionArguments<typeof functions.symbol>
export type SymbolReturn = FunctionReturn<typeof functions.symbol>

export type UpdateOperatorParams = FunctionArguments<typeof functions.updateOperator>
export type UpdateOperatorReturn = FunctionReturn<typeof functions.updateOperator>

export type SetApprovalForAllParams = FunctionArguments<typeof functions.setApprovalForAll>
export type SetApprovalForAllReturn = FunctionReturn<typeof functions.setApprovalForAll>

export type SafeTransferFromParams_1 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>
export type SafeTransferFromReturn_1 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>

export type EstateRegistryParams = FunctionArguments<typeof functions.estateRegistry>
export type EstateRegistryReturn = FunctionReturn<typeof functions.estateRegistry>

export type IsApprovedForAllParams = FunctionArguments<typeof functions.isApprovedForAll>
export type IsApprovedForAllReturn = FunctionReturn<typeof functions.isApprovedForAll>

export type GetApprovedAddressParams = FunctionArguments<typeof functions.getApprovedAddress>
export type GetApprovedAddressReturn = FunctionReturn<typeof functions.getApprovedAddress>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type InitializeParams = FunctionArguments<typeof functions.initialize>
export type InitializeReturn = FunctionReturn<typeof functions.initialize>

export type IsUpdateAuthorizedParams = FunctionArguments<typeof functions.isUpdateAuthorized>
export type IsUpdateAuthorizedReturn = FunctionReturn<typeof functions.isUpdateAuthorized>

export type AuthorizeDeployParams = FunctionArguments<typeof functions.authorizeDeploy>
export type AuthorizeDeployReturn = FunctionReturn<typeof functions.authorizeDeploy>

export type ForbidDeployParams = FunctionArguments<typeof functions.forbidDeploy>
export type ForbidDeployReturn = FunctionReturn<typeof functions.forbidDeploy>

export type AssignNewParcelParams = FunctionArguments<typeof functions.assignNewParcel>
export type AssignNewParcelReturn = FunctionReturn<typeof functions.assignNewParcel>

export type AssignMultipleParcelsParams = FunctionArguments<typeof functions.assignMultipleParcels>
export type AssignMultipleParcelsReturn = FunctionReturn<typeof functions.assignMultipleParcels>

export type PingParams = FunctionArguments<typeof functions.ping>
export type PingReturn = FunctionReturn<typeof functions.ping>

export type SetLatestToNowParams = FunctionArguments<typeof functions.setLatestToNow>
export type SetLatestToNowReturn = FunctionReturn<typeof functions.setLatestToNow>

export type EncodeTokenIdParams = FunctionArguments<typeof functions.encodeTokenId>
export type EncodeTokenIdReturn = FunctionReturn<typeof functions.encodeTokenId>

export type DecodeTokenIdParams = FunctionArguments<typeof functions.decodeTokenId>
export type DecodeTokenIdReturn = FunctionReturn<typeof functions.decodeTokenId>

export type ExistsParams_0 = FunctionArguments<typeof functions['exists(int256,int256)']>
export type ExistsReturn_0 = FunctionReturn<typeof functions['exists(int256,int256)']>

export type ExistsParams_1 = FunctionArguments<typeof functions['exists(uint256)']>
export type ExistsReturn_1 = FunctionReturn<typeof functions['exists(uint256)']>

export type OwnerOfLandParams = FunctionArguments<typeof functions.ownerOfLand>
export type OwnerOfLandReturn = FunctionReturn<typeof functions.ownerOfLand>

export type OwnerOfLandManyParams = FunctionArguments<typeof functions.ownerOfLandMany>
export type OwnerOfLandManyReturn = FunctionReturn<typeof functions.ownerOfLandMany>

export type LandOfParams = FunctionArguments<typeof functions.landOf>
export type LandOfReturn = FunctionReturn<typeof functions.landOf>

export type TokenMetadataParams = FunctionArguments<typeof functions.tokenMetadata>
export type TokenMetadataReturn = FunctionReturn<typeof functions.tokenMetadata>

export type LandDataParams = FunctionArguments<typeof functions.landData>
export type LandDataReturn = FunctionReturn<typeof functions.landData>

export type TransferFromParams = FunctionArguments<typeof functions.transferFrom>
export type TransferFromReturn = FunctionReturn<typeof functions.transferFrom>

export type TransferLandParams = FunctionArguments<typeof functions.transferLand>
export type TransferLandReturn = FunctionReturn<typeof functions.transferLand>

export type TransferManyLandParams = FunctionArguments<typeof functions.transferManyLand>
export type TransferManyLandReturn = FunctionReturn<typeof functions.transferManyLand>

export type TransferLandToEstateParams = FunctionArguments<typeof functions.transferLandToEstate>
export type TransferLandToEstateReturn = FunctionReturn<typeof functions.transferLandToEstate>

export type TransferManyLandToEstateParams = FunctionArguments<typeof functions.transferManyLandToEstate>
export type TransferManyLandToEstateReturn = FunctionReturn<typeof functions.transferManyLandToEstate>

export type SetUpdateOperatorParams = FunctionArguments<typeof functions.setUpdateOperator>
export type SetUpdateOperatorReturn = FunctionReturn<typeof functions.setUpdateOperator>

export type SetUpdateManagerParams = FunctionArguments<typeof functions.setUpdateManager>
export type SetUpdateManagerReturn = FunctionReturn<typeof functions.setUpdateManager>

export type SetEstateRegistryParams = FunctionArguments<typeof functions.setEstateRegistry>
export type SetEstateRegistryReturn = FunctionReturn<typeof functions.setEstateRegistry>

export type CreateEstateParams = FunctionArguments<typeof functions.createEstate>
export type CreateEstateReturn = FunctionReturn<typeof functions.createEstate>

export type CreateEstateWithMetadataParams = FunctionArguments<typeof functions.createEstateWithMetadata>
export type CreateEstateWithMetadataReturn = FunctionReturn<typeof functions.createEstateWithMetadata>

export type UpdateLandDataParams = FunctionArguments<typeof functions.updateLandData>
export type UpdateLandDataReturn = FunctionReturn<typeof functions.updateLandData>

export type UpdateManyLandDataParams = FunctionArguments<typeof functions.updateManyLandData>
export type UpdateManyLandDataReturn = FunctionReturn<typeof functions.updateManyLandData>

