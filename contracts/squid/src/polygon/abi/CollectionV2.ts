import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    AddItem: event("0x1464db75caec2c4f1b56b5246ef01a229ff5edc3b0fcf9b2953fec53b3987d8f", "AddItem(uint256,(string,uint256,uint256,uint256,address,string,string))", {"_itemId": indexed(p.uint256), "_item": p.struct({"rarity": p.string, "maxSupply": p.uint256, "totalSupply": p.uint256, "price": p.uint256, "beneficiary": p.address, "metadata": p.string, "contentHash": p.string})}),
    Approval: event("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)", {"owner": indexed(p.address), "approved": indexed(p.address), "tokenId": indexed(p.uint256)}),
    ApprovalForAll: event("0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31", "ApprovalForAll(address,address,bool)", {"owner": indexed(p.address), "operator": indexed(p.address), "approved": p.bool}),
    BaseURI: event("0xb8fdf10126d507f6daf46465ec25a2bbc08449cf6c944c98219264161391040a", "BaseURI(string,string)", {"_oldBaseURI": p.string, "_newBaseURI": p.string}),
    Complete: event("0x01b7dcb42d49142a99e4c98da755263c600213a33b780986779405b9823501d3", "Complete()", {}),
    CreatorshipTransferred: event("0xf25c6226b06a6bf4b2c2391bb66962bb4290c323cf8b5814efb185aba3f42d5f", "CreatorshipTransferred(address,address)", {"_previousCreator": indexed(p.address), "_newCreator": indexed(p.address)}),
    Issue: event("0x57e2fe3f4d57a597e655a893f43c395d7dc18cb9c9c1898eda75cc0ad7608525", "Issue(address,uint256,uint256,uint256,address)", {"_beneficiary": indexed(p.address), "_tokenId": indexed(p.uint256), "_itemId": indexed(p.uint256), "_issuedId": p.uint256, "_caller": p.address}),
    MetaTransactionExecuted: event("0x5845892132946850460bff5a0083f71031bc5bf9aadcd40f1de79423eac9b10b", "MetaTransactionExecuted(address,address,bytes)", {"userAddress": p.address, "relayerAddress": p.address, "functionSignature": p.bytes}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    RescueItem: event("0x87a972ab2db2d47a0bbefe72cefc4fe5a38b1b9d2bc4b9f366b59fdb6dbd9581", "RescueItem(uint256,string,string)", {"_itemId": indexed(p.uint256), "_contentHash": p.string, "_metadata": p.string}),
    SetApproved: event("0x9ea3c7bd2b8ff87a3768286a529f5932ca385584c8ddf42eea8eade2ac1fb121", "SetApproved(bool,bool)", {"_previousValue": p.bool, "_newValue": p.bool}),
    SetEditable: event("0xc8ca126810b77bfdcb10ae82ef3bc72619d404f9e3c4bf2057f03f057dcf230a", "SetEditable(bool,bool)", {"_previousValue": p.bool, "_newValue": p.bool}),
    SetGlobalManager: event("0x68bb2af98a57ecc88afb707f2d75e1e253c3e30191071548355731e21322a432", "SetGlobalManager(address,bool)", {"_manager": indexed(p.address), "_value": p.bool}),
    SetGlobalMinter: event("0x27defe4b442489e757345f4f743096c5db8d2399e1474bb9653118cdf5313869", "SetGlobalMinter(address,bool)", {"_minter": indexed(p.address), "_value": p.bool}),
    SetItemManager: event("0x1271b13cf757efd2a177fe5f26c2111b69ce904f1d0e3372832dc4e4773298cc", "SetItemManager(uint256,address,bool)", {"_itemId": indexed(p.uint256), "_manager": indexed(p.address), "_value": p.bool}),
    SetItemMinter: event("0x45a634291439a8ec187e2b70279696615889c34c17c3a1208a7f85544e13b565", "SetItemMinter(uint256,address,uint256)", {"_itemId": indexed(p.uint256), "_minter": indexed(p.address), "_value": p.uint256}),
    Transfer: event("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)", {"from": indexed(p.address), "to": indexed(p.address), "tokenId": indexed(p.uint256)}),
    UpdateItemData: event("0xc3835f7dd5063bbc8fdc7077944498159008d73d30bf55fe6513e683c8c38209", "UpdateItemData(uint256,uint256,address,string)", {"_itemId": indexed(p.uint256), "_price": p.uint256, "_beneficiary": p.address, "_metadata": p.string}),
}

export const functions = {
    COLLECTION_HASH: viewFun("0x7e12acce", "COLLECTION_HASH()", {}, p.bytes32),
    ISSUED_ID_BITS: viewFun("0x950fa904", "ISSUED_ID_BITS()", {}, p.uint8),
    ITEM_ID_BITS: viewFun("0x5ec7c537", "ITEM_ID_BITS()", {}, p.uint8),
    MAX_ISSUED_ID: viewFun("0xf6f07b63", "MAX_ISSUED_ID()", {}, p.uint216),
    MAX_ITEM_ID: viewFun("0x48622018", "MAX_ITEM_ID()", {}, p.uint40),
    addItems: fun("0x425f60ed", "addItems((string,uint256,address,string)[])", {"_items": p.array(p.struct({"rarity": p.string, "price": p.uint256, "beneficiary": p.address, "metadata": p.string}))}, ),
    approve: fun("0x095ea7b3", "approve(address,uint256)", {"to": p.address, "tokenId": p.uint256}, ),
    balanceOf: viewFun("0x70a08231", "balanceOf(address)", {"owner": p.address}, p.uint256),
    baseURI: viewFun("0x6c0360eb", "baseURI()", {}, p.string),
    batchTransferFrom: fun("0xf3993d11", "batchTransferFrom(address,address,uint256[])", {"_from": p.address, "_to": p.address, "_tokenIds": p.array(p.uint256)}, ),
    completeCollection: fun("0xf59d9e3f", "completeCollection()", {}, ),
    createdAt: viewFun("0xcf09e0d0", "createdAt()", {}, p.uint256),
    creator: viewFun("0x02d05d3f", "creator()", {}, p.address),
    decodeTokenId: viewFun("0x7efd9112", "decodeTokenId(uint256)", {"_id": p.uint256}, {"itemId": p.uint256, "issuedId": p.uint256}),
    domainSeparator: viewFun("0xf698da25", "domainSeparator()", {}, p.bytes32),
    editItemsData: fun("0x0ebde06e", "editItemsData(uint256[],uint256[],address[],string[])", {"_itemIds": p.array(p.uint256), "_prices": p.array(p.uint256), "_beneficiaries": p.array(p.address), "_metadatas": p.array(p.string)}, ),
    encodeTokenId: viewFun("0xebd46d64", "encodeTokenId(uint256,uint256)", {"_itemId": p.uint256, "_issuedId": p.uint256}, p.uint256),
    executeMetaTransaction: fun("0x0c53c51c", "executeMetaTransaction(address,bytes,bytes32,bytes32,uint8)", {"userAddress": p.address, "functionSignature": p.bytes, "sigR": p.bytes32, "sigS": p.bytes32, "sigV": p.uint8}, p.bytes),
    getApproved: viewFun("0x081812fc", "getApproved(uint256)", {"tokenId": p.uint256}, p.address),
    getChainId: viewFun("0x3408e470", "getChainId()", {}, p.uint256),
    getNonce: viewFun("0x2d0335ab", "getNonce(address)", {"user": p.address}, p.uint256),
    globalManagers: viewFun("0x7682dfca", "globalManagers(address)", {"_0": p.address}, p.bool),
    globalMinters: viewFun("0xb4c2025e", "globalMinters(address)", {"_0": p.address}, p.bool),
    initImplementation: fun("0x30127574", "initImplementation()", {}, ),
    initialize: fun("0xd14fee77", "initialize(string,string,string,address,bool,bool,address,(string,uint256,address,string)[])", {"_name": p.string, "_symbol": p.string, "_baseURI": p.string, "_creator": p.address, "_shouldComplete": p.bool, "_isApproved": p.bool, "_rarities": p.address, "_items": p.array(p.struct({"rarity": p.string, "price": p.uint256, "beneficiary": p.address, "metadata": p.string}))}, ),
    isApproved: viewFun("0x28f371aa", "isApproved()", {}, p.bool),
    isApprovedForAll: viewFun("0xe985e9c5", "isApprovedForAll(address,address)", {"owner": p.address, "operator": p.address}, p.bool),
    isCompleted: viewFun("0xfa391c64", "isCompleted()", {}, p.bool),
    isEditable: viewFun("0xb4da4e37", "isEditable()", {}, p.bool),
    isInitialized: viewFun("0x392e53cd", "isInitialized()", {}, p.bool),
    isMintingAllowed: viewFun("0x2b481883", "isMintingAllowed()", {}, p.bool),
    issueTokens: fun("0x7c8f76a1", "issueTokens(address[],uint256[])", {"_beneficiaries": p.array(p.address), "_itemIds": p.array(p.uint256)}, ),
    itemManagers: viewFun("0xbe4763b3", "itemManagers(uint256,address)", {"_0": p.uint256, "_1": p.address}, p.bool),
    itemMinters: viewFun("0x6d67b1ab", "itemMinters(uint256,address)", {"_0": p.uint256, "_1": p.address}, p.uint256),
    items: viewFun("0xbfb231d2", "items(uint256)", {"_0": p.uint256}, {"rarity": p.string, "maxSupply": p.uint256, "totalSupply": p.uint256, "price": p.uint256, "beneficiary": p.address, "metadata": p.string, "contentHash": p.string}),
    itemsCount: viewFun("0xe2c03ace", "itemsCount()", {}, p.uint256),
    name: viewFun("0x06fdde03", "name()", {}, p.string),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    ownerOf: viewFun("0x6352211e", "ownerOf(uint256)", {"tokenId": p.uint256}, p.address),
    rarities: viewFun("0x3992cd0d", "rarities()", {}, p.address),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    rescueItems: fun("0x3c963655", "rescueItems(uint256[],string[],string[])", {"_itemIds": p.array(p.uint256), "_contentHashes": p.array(p.string), "_metadatas": p.array(p.string)}, ),
    safeBatchTransferFrom: fun("0x28cfbd46", "safeBatchTransferFrom(address,address,uint256[],bytes)", {"_from": p.address, "_to": p.address, "_tokenIds": p.array(p.uint256), "_data": p.bytes}, ),
    'safeTransferFrom(address,address,uint256)': fun("0x42842e0e", "safeTransferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "tokenId": p.uint256}, ),
    'safeTransferFrom(address,address,uint256,bytes)': fun("0xb88d4fde", "safeTransferFrom(address,address,uint256,bytes)", {"from": p.address, "to": p.address, "tokenId": p.uint256, "_data": p.bytes}, ),
    setApprovalForAll: fun("0xa22cb465", "setApprovalForAll(address,bool)", {"operator": p.address, "approved": p.bool}, ),
    setApproved: fun("0x46d5a568", "setApproved(bool)", {"_value": p.bool}, ),
    setBaseURI: fun("0x55f804b3", "setBaseURI(string)", {"_baseURI": p.string}, ),
    setEditable: fun("0x2cb0d48a", "setEditable(bool)", {"_value": p.bool}, ),
    setItemsManagers: fun("0x239a603b", "setItemsManagers(uint256[],address[],bool[])", {"_itemIds": p.array(p.uint256), "_managers": p.array(p.address), "_values": p.array(p.bool)}, ),
    setItemsMinters: fun("0xe6170a53", "setItemsMinters(uint256[],address[],uint256[])", {"_itemIds": p.array(p.uint256), "_minters": p.array(p.address), "_values": p.array(p.uint256)}, ),
    setManagers: fun("0x3bdcc923", "setManagers(address[],bool[])", {"_managers": p.array(p.address), "_values": p.array(p.bool)}, ),
    setMinters: fun("0x41bceced", "setMinters(address[],bool[])", {"_minters": p.array(p.address), "_values": p.array(p.bool)}, ),
    supportsInterface: viewFun("0x01ffc9a7", "supportsInterface(bytes4)", {"interfaceId": p.bytes4}, p.bool),
    symbol: viewFun("0x95d89b41", "symbol()", {}, p.string),
    tokenByIndex: viewFun("0x4f6ccce7", "tokenByIndex(uint256)", {"index": p.uint256}, p.uint256),
    tokenOfOwnerByIndex: viewFun("0x2f745c59", "tokenOfOwnerByIndex(address,uint256)", {"owner": p.address, "index": p.uint256}, p.uint256),
    tokenURI: viewFun("0xc87b56dd", "tokenURI(uint256)", {"_tokenId": p.uint256}, p.string),
    totalSupply: viewFun("0x18160ddd", "totalSupply()", {}, p.uint256),
    transferCreatorship: fun("0x6d2e4b1b", "transferCreatorship(address)", {"_newCreator": p.address}, ),
    transferFrom: fun("0x23b872dd", "transferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "tokenId": p.uint256}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
}

export class Contract extends ContractBase {

    COLLECTION_HASH() {
        return this.eth_call(functions.COLLECTION_HASH, {})
    }

    ISSUED_ID_BITS() {
        return this.eth_call(functions.ISSUED_ID_BITS, {})
    }

    ITEM_ID_BITS() {
        return this.eth_call(functions.ITEM_ID_BITS, {})
    }

    MAX_ISSUED_ID() {
        return this.eth_call(functions.MAX_ISSUED_ID, {})
    }

    MAX_ITEM_ID() {
        return this.eth_call(functions.MAX_ITEM_ID, {})
    }

    balanceOf(owner: BalanceOfParams["owner"]) {
        return this.eth_call(functions.balanceOf, {owner})
    }

    baseURI() {
        return this.eth_call(functions.baseURI, {})
    }

    createdAt() {
        return this.eth_call(functions.createdAt, {})
    }

    creator() {
        return this.eth_call(functions.creator, {})
    }

    decodeTokenId(_id: DecodeTokenIdParams["_id"]) {
        return this.eth_call(functions.decodeTokenId, {_id})
    }

    domainSeparator() {
        return this.eth_call(functions.domainSeparator, {})
    }

    encodeTokenId(_itemId: EncodeTokenIdParams["_itemId"], _issuedId: EncodeTokenIdParams["_issuedId"]) {
        return this.eth_call(functions.encodeTokenId, {_itemId, _issuedId})
    }

    getApproved(tokenId: GetApprovedParams["tokenId"]) {
        return this.eth_call(functions.getApproved, {tokenId})
    }

    getChainId() {
        return this.eth_call(functions.getChainId, {})
    }

    getNonce(user: GetNonceParams["user"]) {
        return this.eth_call(functions.getNonce, {user})
    }

    globalManagers(_0: GlobalManagersParams["_0"]) {
        return this.eth_call(functions.globalManagers, {_0})
    }

    globalMinters(_0: GlobalMintersParams["_0"]) {
        return this.eth_call(functions.globalMinters, {_0})
    }

    isApproved() {
        return this.eth_call(functions.isApproved, {})
    }

    isApprovedForAll(owner: IsApprovedForAllParams["owner"], operator: IsApprovedForAllParams["operator"]) {
        return this.eth_call(functions.isApprovedForAll, {owner, operator})
    }

    isCompleted() {
        return this.eth_call(functions.isCompleted, {})
    }

    isEditable() {
        return this.eth_call(functions.isEditable, {})
    }

    isInitialized() {
        return this.eth_call(functions.isInitialized, {})
    }

    isMintingAllowed() {
        return this.eth_call(functions.isMintingAllowed, {})
    }

    itemManagers(_0: ItemManagersParams["_0"], _1: ItemManagersParams["_1"]) {
        return this.eth_call(functions.itemManagers, {_0, _1})
    }

    itemMinters(_0: ItemMintersParams["_0"], _1: ItemMintersParams["_1"]) {
        return this.eth_call(functions.itemMinters, {_0, _1})
    }

    items(_0: ItemsParams["_0"]) {
        return this.eth_call(functions.items, {_0})
    }

    itemsCount() {
        return this.eth_call(functions.itemsCount, {})
    }

    name() {
        return this.eth_call(functions.name, {})
    }

    owner() {
        return this.eth_call(functions.owner, {})
    }

    ownerOf(tokenId: OwnerOfParams["tokenId"]) {
        return this.eth_call(functions.ownerOf, {tokenId})
    }

    rarities() {
        return this.eth_call(functions.rarities, {})
    }

    supportsInterface(interfaceId: SupportsInterfaceParams["interfaceId"]) {
        return this.eth_call(functions.supportsInterface, {interfaceId})
    }

    symbol() {
        return this.eth_call(functions.symbol, {})
    }

    tokenByIndex(index: TokenByIndexParams["index"]) {
        return this.eth_call(functions.tokenByIndex, {index})
    }

    tokenOfOwnerByIndex(owner: TokenOfOwnerByIndexParams["owner"], index: TokenOfOwnerByIndexParams["index"]) {
        return this.eth_call(functions.tokenOfOwnerByIndex, {owner, index})
    }

    tokenURI(_tokenId: TokenURIParams["_tokenId"]) {
        return this.eth_call(functions.tokenURI, {_tokenId})
    }

    totalSupply() {
        return this.eth_call(functions.totalSupply, {})
    }
}

/// Event types
export type AddItemEventArgs = EParams<typeof events.AddItem>
export type ApprovalEventArgs = EParams<typeof events.Approval>
export type ApprovalForAllEventArgs = EParams<typeof events.ApprovalForAll>
export type BaseURIEventArgs = EParams<typeof events.BaseURI>
export type CompleteEventArgs = EParams<typeof events.Complete>
export type CreatorshipTransferredEventArgs = EParams<typeof events.CreatorshipTransferred>
export type IssueEventArgs = EParams<typeof events.Issue>
export type MetaTransactionExecutedEventArgs = EParams<typeof events.MetaTransactionExecuted>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type RescueItemEventArgs = EParams<typeof events.RescueItem>
export type SetApprovedEventArgs = EParams<typeof events.SetApproved>
export type SetEditableEventArgs = EParams<typeof events.SetEditable>
export type SetGlobalManagerEventArgs = EParams<typeof events.SetGlobalManager>
export type SetGlobalMinterEventArgs = EParams<typeof events.SetGlobalMinter>
export type SetItemManagerEventArgs = EParams<typeof events.SetItemManager>
export type SetItemMinterEventArgs = EParams<typeof events.SetItemMinter>
export type TransferEventArgs = EParams<typeof events.Transfer>
export type UpdateItemDataEventArgs = EParams<typeof events.UpdateItemData>

/// Function types
export type COLLECTION_HASHParams = FunctionArguments<typeof functions.COLLECTION_HASH>
export type COLLECTION_HASHReturn = FunctionReturn<typeof functions.COLLECTION_HASH>

export type ISSUED_ID_BITSParams = FunctionArguments<typeof functions.ISSUED_ID_BITS>
export type ISSUED_ID_BITSReturn = FunctionReturn<typeof functions.ISSUED_ID_BITS>

export type ITEM_ID_BITSParams = FunctionArguments<typeof functions.ITEM_ID_BITS>
export type ITEM_ID_BITSReturn = FunctionReturn<typeof functions.ITEM_ID_BITS>

export type MAX_ISSUED_IDParams = FunctionArguments<typeof functions.MAX_ISSUED_ID>
export type MAX_ISSUED_IDReturn = FunctionReturn<typeof functions.MAX_ISSUED_ID>

export type MAX_ITEM_IDParams = FunctionArguments<typeof functions.MAX_ITEM_ID>
export type MAX_ITEM_IDReturn = FunctionReturn<typeof functions.MAX_ITEM_ID>

export type AddItemsParams = FunctionArguments<typeof functions.addItems>
export type AddItemsReturn = FunctionReturn<typeof functions.addItems>

export type ApproveParams = FunctionArguments<typeof functions.approve>
export type ApproveReturn = FunctionReturn<typeof functions.approve>

export type BalanceOfParams = FunctionArguments<typeof functions.balanceOf>
export type BalanceOfReturn = FunctionReturn<typeof functions.balanceOf>

export type BaseURIParams = FunctionArguments<typeof functions.baseURI>
export type BaseURIReturn = FunctionReturn<typeof functions.baseURI>

export type BatchTransferFromParams = FunctionArguments<typeof functions.batchTransferFrom>
export type BatchTransferFromReturn = FunctionReturn<typeof functions.batchTransferFrom>

export type CompleteCollectionParams = FunctionArguments<typeof functions.completeCollection>
export type CompleteCollectionReturn = FunctionReturn<typeof functions.completeCollection>

export type CreatedAtParams = FunctionArguments<typeof functions.createdAt>
export type CreatedAtReturn = FunctionReturn<typeof functions.createdAt>

export type CreatorParams = FunctionArguments<typeof functions.creator>
export type CreatorReturn = FunctionReturn<typeof functions.creator>

export type DecodeTokenIdParams = FunctionArguments<typeof functions.decodeTokenId>
export type DecodeTokenIdReturn = FunctionReturn<typeof functions.decodeTokenId>

export type DomainSeparatorParams = FunctionArguments<typeof functions.domainSeparator>
export type DomainSeparatorReturn = FunctionReturn<typeof functions.domainSeparator>

export type EditItemsDataParams = FunctionArguments<typeof functions.editItemsData>
export type EditItemsDataReturn = FunctionReturn<typeof functions.editItemsData>

export type EncodeTokenIdParams = FunctionArguments<typeof functions.encodeTokenId>
export type EncodeTokenIdReturn = FunctionReturn<typeof functions.encodeTokenId>

export type ExecuteMetaTransactionParams = FunctionArguments<typeof functions.executeMetaTransaction>
export type ExecuteMetaTransactionReturn = FunctionReturn<typeof functions.executeMetaTransaction>

export type GetApprovedParams = FunctionArguments<typeof functions.getApproved>
export type GetApprovedReturn = FunctionReturn<typeof functions.getApproved>

export type GetChainIdParams = FunctionArguments<typeof functions.getChainId>
export type GetChainIdReturn = FunctionReturn<typeof functions.getChainId>

export type GetNonceParams = FunctionArguments<typeof functions.getNonce>
export type GetNonceReturn = FunctionReturn<typeof functions.getNonce>

export type GlobalManagersParams = FunctionArguments<typeof functions.globalManagers>
export type GlobalManagersReturn = FunctionReturn<typeof functions.globalManagers>

export type GlobalMintersParams = FunctionArguments<typeof functions.globalMinters>
export type GlobalMintersReturn = FunctionReturn<typeof functions.globalMinters>

export type InitImplementationParams = FunctionArguments<typeof functions.initImplementation>
export type InitImplementationReturn = FunctionReturn<typeof functions.initImplementation>

export type InitializeParams = FunctionArguments<typeof functions.initialize>
export type InitializeReturn = FunctionReturn<typeof functions.initialize>

export type IsApprovedParams = FunctionArguments<typeof functions.isApproved>
export type IsApprovedReturn = FunctionReturn<typeof functions.isApproved>

export type IsApprovedForAllParams = FunctionArguments<typeof functions.isApprovedForAll>
export type IsApprovedForAllReturn = FunctionReturn<typeof functions.isApprovedForAll>

export type IsCompletedParams = FunctionArguments<typeof functions.isCompleted>
export type IsCompletedReturn = FunctionReturn<typeof functions.isCompleted>

export type IsEditableParams = FunctionArguments<typeof functions.isEditable>
export type IsEditableReturn = FunctionReturn<typeof functions.isEditable>

export type IsInitializedParams = FunctionArguments<typeof functions.isInitialized>
export type IsInitializedReturn = FunctionReturn<typeof functions.isInitialized>

export type IsMintingAllowedParams = FunctionArguments<typeof functions.isMintingAllowed>
export type IsMintingAllowedReturn = FunctionReturn<typeof functions.isMintingAllowed>

export type IssueTokensParams = FunctionArguments<typeof functions.issueTokens>
export type IssueTokensReturn = FunctionReturn<typeof functions.issueTokens>

export type ItemManagersParams = FunctionArguments<typeof functions.itemManagers>
export type ItemManagersReturn = FunctionReturn<typeof functions.itemManagers>

export type ItemMintersParams = FunctionArguments<typeof functions.itemMinters>
export type ItemMintersReturn = FunctionReturn<typeof functions.itemMinters>

export type ItemsParams = FunctionArguments<typeof functions.items>
export type ItemsReturn = FunctionReturn<typeof functions.items>

export type ItemsCountParams = FunctionArguments<typeof functions.itemsCount>
export type ItemsCountReturn = FunctionReturn<typeof functions.itemsCount>

export type NameParams = FunctionArguments<typeof functions.name>
export type NameReturn = FunctionReturn<typeof functions.name>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type OwnerOfParams = FunctionArguments<typeof functions.ownerOf>
export type OwnerOfReturn = FunctionReturn<typeof functions.ownerOf>

export type RaritiesParams = FunctionArguments<typeof functions.rarities>
export type RaritiesReturn = FunctionReturn<typeof functions.rarities>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type RescueItemsParams = FunctionArguments<typeof functions.rescueItems>
export type RescueItemsReturn = FunctionReturn<typeof functions.rescueItems>

export type SafeBatchTransferFromParams = FunctionArguments<typeof functions.safeBatchTransferFrom>
export type SafeBatchTransferFromReturn = FunctionReturn<typeof functions.safeBatchTransferFrom>

export type SafeTransferFromParams_0 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256)']>
export type SafeTransferFromReturn_0 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256)']>

export type SafeTransferFromParams_1 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>
export type SafeTransferFromReturn_1 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>

export type SetApprovalForAllParams = FunctionArguments<typeof functions.setApprovalForAll>
export type SetApprovalForAllReturn = FunctionReturn<typeof functions.setApprovalForAll>

export type SetApprovedParams = FunctionArguments<typeof functions.setApproved>
export type SetApprovedReturn = FunctionReturn<typeof functions.setApproved>

export type SetBaseURIParams = FunctionArguments<typeof functions.setBaseURI>
export type SetBaseURIReturn = FunctionReturn<typeof functions.setBaseURI>

export type SetEditableParams = FunctionArguments<typeof functions.setEditable>
export type SetEditableReturn = FunctionReturn<typeof functions.setEditable>

export type SetItemsManagersParams = FunctionArguments<typeof functions.setItemsManagers>
export type SetItemsManagersReturn = FunctionReturn<typeof functions.setItemsManagers>

export type SetItemsMintersParams = FunctionArguments<typeof functions.setItemsMinters>
export type SetItemsMintersReturn = FunctionReturn<typeof functions.setItemsMinters>

export type SetManagersParams = FunctionArguments<typeof functions.setManagers>
export type SetManagersReturn = FunctionReturn<typeof functions.setManagers>

export type SetMintersParams = FunctionArguments<typeof functions.setMinters>
export type SetMintersReturn = FunctionReturn<typeof functions.setMinters>

export type SupportsInterfaceParams = FunctionArguments<typeof functions.supportsInterface>
export type SupportsInterfaceReturn = FunctionReturn<typeof functions.supportsInterface>

export type SymbolParams = FunctionArguments<typeof functions.symbol>
export type SymbolReturn = FunctionReturn<typeof functions.symbol>

export type TokenByIndexParams = FunctionArguments<typeof functions.tokenByIndex>
export type TokenByIndexReturn = FunctionReturn<typeof functions.tokenByIndex>

export type TokenOfOwnerByIndexParams = FunctionArguments<typeof functions.tokenOfOwnerByIndex>
export type TokenOfOwnerByIndexReturn = FunctionReturn<typeof functions.tokenOfOwnerByIndex>

export type TokenURIParams = FunctionArguments<typeof functions.tokenURI>
export type TokenURIReturn = FunctionReturn<typeof functions.tokenURI>

export type TotalSupplyParams = FunctionArguments<typeof functions.totalSupply>
export type TotalSupplyReturn = FunctionReturn<typeof functions.totalSupply>

export type TransferCreatorshipParams = FunctionArguments<typeof functions.transferCreatorship>
export type TransferCreatorshipReturn = FunctionReturn<typeof functions.transferCreatorship>

export type TransferFromParams = FunctionArguments<typeof functions.transferFrom>
export type TransferFromReturn = FunctionReturn<typeof functions.transferFrom>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

