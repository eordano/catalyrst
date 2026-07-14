import * as p from '@subsquid/evm-codec'
import { event, fun, viewFun, indexed, ContractBase } from '@subsquid/evm-abi'
import type { EventParams as EParams, FunctionArguments, FunctionReturn } from '@subsquid/evm-abi'

export const events = {
    Approval: event("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)", {"owner": indexed(p.address), "approved": indexed(p.address), "tokenId": indexed(p.uint256)}),
    ApprovalForAll: event("0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31", "ApprovalForAll(address,address,bool)", {"owner": indexed(p.address), "operator": indexed(p.address), "approved": p.bool}),
    BaseURI: event("0xb8fdf10126d507f6daf46465ec25a2bbc08449cf6c944c98219264161391040a", "BaseURI(string,string)", {"_oldBaseURI": p.string, "_newBaseURI": p.string}),
    BaseUpdated: event("0x1a60ded2578adf94e0523d41e22df9bccb7da384ab33deed06b6dccd6a9798a1", "BaseUpdated(address,address)", {"_previousBase": indexed(p.address), "_newBase": indexed(p.address)}),
    CallForwarwedToResolver: event("0x3a321a1276dce90a83e15ae23e7fa1d89b4b71bfcdd5db994f1a1324bbab4dd4", "CallForwarwedToResolver(address,bytes,bytes)", {"_resolver": indexed(p.address), "_data": p.bytes, "res": p.bytes}),
    ControllerAdded: event("0x0a8bb31534c0ed46f380cb867bd5c803a189ced9a764e30b3a4991a9901d7474", "ControllerAdded(address)", {"_controller": indexed(p.address)}),
    ControllerRemoved: event("0x33d83959be2573f5453b12eb9d43b3499bc57d96bd2f067ba44803c859e81113", "ControllerRemoved(address)", {"_controller": indexed(p.address)}),
    DomainReclaimed: event("0x3a11d88ee5aca155d3f605ff73bba91616741610a8f88d51d2fa9da8c9a89dbd", "DomainReclaimed(uint256)", {"_tokenId": indexed(p.uint256)}),
    DomainTransferred: event("0x8abf792cabfeedb418c98e537e6891e54301c260f8b7908300627771510054b1", "DomainTransferred(address,uint256)", {"_newOwner": indexed(p.address), "_tokenId": indexed(p.uint256)}),
    MigrationFinished: event("0xceab6b91af27f4253aa8bd4ee8179c32d60bede7297c333dcb56de2641c05544", "MigrationFinished()", {}),
    NameRegistered: event("0x570313dae523ecb48b1176a4b60272e5ea7ec637f5b2d09983cbc4bf25e7e9e3", "NameRegistered(address,address,bytes32,string,uint256)", {"_caller": indexed(p.address), "_beneficiary": indexed(p.address), "_labelHash": indexed(p.bytes32), "_subdomain": p.string, "_createdDate": p.uint256}),
    OwnershipTransferred: event("0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", {"previousOwner": indexed(p.address), "newOwner": indexed(p.address)}),
    Reclaimed: event("0xc4cc5c1b6cf3b5cafd06bc7fa8b6320dbeea074c4f18c4c036e52a3a773aac54", "Reclaimed(address,address,uint256)", {"_caller": indexed(p.address), "_owner": indexed(p.address), "_tokenId": indexed(p.uint256)}),
    RegistryUpdated: event("0x482b97c53e48ffa324a976e2738053e9aff6eee04d8aac63b10e19411d869b82", "RegistryUpdated(address,address)", {"_previousRegistry": indexed(p.address), "_newRegistry": indexed(p.address)}),
    ResolverUpdated: event("0x84b83d2b66cac119ccaaca68b476b0dc5371d5f2fd27f697770a910175fd38b6", "ResolverUpdated(address,address)", {"_oldResolver": indexed(p.address), "_newResolver": indexed(p.address)}),
    Transfer: event("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)", {"from": indexed(p.address), "to": indexed(p.address), "tokenId": indexed(p.uint256)}),
}

export const functions = {
    ERC721_RECEIVED: viewFun("0xecc98ce4", "ERC721_RECEIVED()", {}, p.bytes4),
    addController: fun("0xa7fc7a07", "addController(address)", {"controller": p.address}, ),
    approve: fun("0x095ea7b3", "approve(address,uint256)", {"to": p.address, "tokenId": p.uint256}, ),
    available: viewFun("0xaeb8ce9b", "available(string)", {"_subdomain": p.string}, p.bool),
    balanceOf: viewFun("0x70a08231", "balanceOf(address)", {"owner": p.address}, p.uint256),
    base: viewFun("0x5001f3b5", "base()", {}, p.address),
    baseURI: viewFun("0x6c0360eb", "baseURI()", {}, p.string),
    controllers: viewFun("0xda8c229e", "controllers(address)", {"_0": p.address}, p.bool),
    domain: viewFun("0xc2fb26a6", "domain()", {}, p.string),
    domainNameHash: viewFun("0xca53db9d", "domainNameHash()", {}, p.bytes32),
    forwardToResolver: fun("0xc28f48ce", "forwardToResolver(bytes)", {"_data": p.bytes}, ),
    getApproved: viewFun("0x081812fc", "getApproved(uint256)", {"tokenId": p.uint256}, p.address),
    getOwnerOf: viewFun("0xbef48ddf", "getOwnerOf(string)", {"_subdomain": p.string}, p.address),
    getTokenId: viewFun("0x1e7663bc", "getTokenId(string)", {"_subdomain": p.string}, p.uint256),
    isApprovedForAll: viewFun("0xe985e9c5", "isApprovedForAll(address,address)", {"owner": p.address, "operator": p.address}, p.bool),
    isOwner: viewFun("0x8f32d59b", "isOwner()", {}, p.bool),
    migrateNames: fun("0x5b81eb88", "migrateNames(bytes32[],address[],uint256[])", {"_names": p.array(p.bytes32), "_beneficiaries": p.array(p.address), "_createdDates": p.array(p.uint256)}, ),
    migrated: viewFun("0x2c678c64", "migrated()", {}, p.bool),
    migrationFinished: fun("0x3f9e23e5", "migrationFinished()", {}, ),
    name: viewFun("0x06fdde03", "name()", {}, p.string),
    onERC721Received: fun("0x150b7a02", "onERC721Received(address,address,uint256,bytes)", {"_0": p.address, "_1": p.address, "_tokenId": p.uint256, "_3": p.bytes}, p.bytes4),
    owner: viewFun("0x8da5cb5b", "owner()", {}, p.address),
    ownerOf: viewFun("0x6352211e", "ownerOf(uint256)", {"tokenId": p.uint256}, p.address),
    'reclaim(uint256,address)': fun("0x28ed4f6c", "reclaim(uint256,address)", {"_tokenId": p.uint256, "_owner": p.address}, ),
    'reclaim(uint256)': fun("0x2dabbeed", "reclaim(uint256)", {"_tokenId": p.uint256}, ),
    reclaimDomain: fun("0x398b93d1", "reclaimDomain(uint256)", {"_tokenId": p.uint256}, ),
    register: fun("0x1e59c529", "register(string,address)", {"_subdomain": p.string, "_beneficiary": p.address}, ),
    registry: viewFun("0x7b103999", "registry()", {}, p.address),
    removeController: fun("0xf6a74ed7", "removeController(address)", {"controller": p.address}, ),
    renounceOwnership: fun("0x715018a6", "renounceOwnership()", {}, ),
    'safeTransferFrom(address,address,uint256)': fun("0x42842e0e", "safeTransferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "tokenId": p.uint256}, ),
    'safeTransferFrom(address,address,uint256,bytes)': fun("0xb88d4fde", "safeTransferFrom(address,address,uint256,bytes)", {"from": p.address, "to": p.address, "tokenId": p.uint256, "_data": p.bytes}, ),
    setApprovalForAll: fun("0xa22cb465", "setApprovalForAll(address,bool)", {"to": p.address, "approved": p.bool}, ),
    setResolver: fun("0x4e543b26", "setResolver(address)", {"_resolver": p.address}, ),
    subdomains: viewFun("0x9d79d081", "subdomains(bytes32)", {"_0": p.bytes32}, p.string),
    supportsInterface: viewFun("0x01ffc9a7", "supportsInterface(bytes4)", {"interfaceId": p.bytes4}, p.bool),
    symbol: viewFun("0x95d89b41", "symbol()", {}, p.string),
    tokenByIndex: viewFun("0x4f6ccce7", "tokenByIndex(uint256)", {"index": p.uint256}, p.uint256),
    tokenOfOwnerByIndex: viewFun("0x2f745c59", "tokenOfOwnerByIndex(address,uint256)", {"owner": p.address, "index": p.uint256}, p.uint256),
    tokenURI: viewFun("0xc87b56dd", "tokenURI(uint256)", {"_tokenId": p.uint256}, p.string),
    topdomain: viewFun("0x24f093a4", "topdomain()", {}, p.string),
    topdomainNameHash: viewFun("0x3777d159", "topdomainNameHash()", {}, p.bytes32),
    totalSupply: viewFun("0x18160ddd", "totalSupply()", {}, p.uint256),
    transferDomainOwnership: fun("0x11498b46", "transferDomainOwnership(address,uint256)", {"_owner": p.address, "_tokenId": p.uint256}, ),
    transferFrom: fun("0x23b872dd", "transferFrom(address,address,uint256)", {"from": p.address, "to": p.address, "tokenId": p.uint256}, ),
    transferOwnership: fun("0xf2fde38b", "transferOwnership(address)", {"newOwner": p.address}, ),
    updateBase: fun("0xcf5ffeba", "updateBase(address)", {"_base": p.address}, ),
    updateBaseURI: fun("0x931688cb", "updateBaseURI(string)", {"_baseURI": p.string}, ),
    updateRegistry: fun("0x1a5da6c8", "updateRegistry(address)", {"_registry": p.address}, ),
}

export class Contract extends ContractBase {

    ERC721_RECEIVED() {
        return this.eth_call(functions.ERC721_RECEIVED, {})
    }

    available(_subdomain: AvailableParams["_subdomain"]) {
        return this.eth_call(functions.available, {_subdomain})
    }

    balanceOf(owner: BalanceOfParams["owner"]) {
        return this.eth_call(functions.balanceOf, {owner})
    }

    base() {
        return this.eth_call(functions.base, {})
    }

    baseURI() {
        return this.eth_call(functions.baseURI, {})
    }

    controllers(_0: ControllersParams["_0"]) {
        return this.eth_call(functions.controllers, {_0})
    }

    domain() {
        return this.eth_call(functions.domain, {})
    }

    domainNameHash() {
        return this.eth_call(functions.domainNameHash, {})
    }

    getApproved(tokenId: GetApprovedParams["tokenId"]) {
        return this.eth_call(functions.getApproved, {tokenId})
    }

    getOwnerOf(_subdomain: GetOwnerOfParams["_subdomain"]) {
        return this.eth_call(functions.getOwnerOf, {_subdomain})
    }

    getTokenId(_subdomain: GetTokenIdParams["_subdomain"]) {
        return this.eth_call(functions.getTokenId, {_subdomain})
    }

    isApprovedForAll(owner: IsApprovedForAllParams["owner"], operator: IsApprovedForAllParams["operator"]) {
        return this.eth_call(functions.isApprovedForAll, {owner, operator})
    }

    isOwner() {
        return this.eth_call(functions.isOwner, {})
    }

    migrated() {
        return this.eth_call(functions.migrated, {})
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

    registry() {
        return this.eth_call(functions.registry, {})
    }

    subdomains(_0: SubdomainsParams["_0"]) {
        return this.eth_call(functions.subdomains, {_0})
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

    topdomain() {
        return this.eth_call(functions.topdomain, {})
    }

    topdomainNameHash() {
        return this.eth_call(functions.topdomainNameHash, {})
    }

    totalSupply() {
        return this.eth_call(functions.totalSupply, {})
    }
}

/// Event types
export type ApprovalEventArgs = EParams<typeof events.Approval>
export type ApprovalForAllEventArgs = EParams<typeof events.ApprovalForAll>
export type BaseURIEventArgs = EParams<typeof events.BaseURI>
export type BaseUpdatedEventArgs = EParams<typeof events.BaseUpdated>
export type CallForwarwedToResolverEventArgs = EParams<typeof events.CallForwarwedToResolver>
export type ControllerAddedEventArgs = EParams<typeof events.ControllerAdded>
export type ControllerRemovedEventArgs = EParams<typeof events.ControllerRemoved>
export type DomainReclaimedEventArgs = EParams<typeof events.DomainReclaimed>
export type DomainTransferredEventArgs = EParams<typeof events.DomainTransferred>
export type MigrationFinishedEventArgs = EParams<typeof events.MigrationFinished>
export type NameRegisteredEventArgs = EParams<typeof events.NameRegistered>
export type OwnershipTransferredEventArgs = EParams<typeof events.OwnershipTransferred>
export type ReclaimedEventArgs = EParams<typeof events.Reclaimed>
export type RegistryUpdatedEventArgs = EParams<typeof events.RegistryUpdated>
export type ResolverUpdatedEventArgs = EParams<typeof events.ResolverUpdated>
export type TransferEventArgs = EParams<typeof events.Transfer>

/// Function types
export type ERC721_RECEIVEDParams = FunctionArguments<typeof functions.ERC721_RECEIVED>
export type ERC721_RECEIVEDReturn = FunctionReturn<typeof functions.ERC721_RECEIVED>

export type AddControllerParams = FunctionArguments<typeof functions.addController>
export type AddControllerReturn = FunctionReturn<typeof functions.addController>

export type ApproveParams = FunctionArguments<typeof functions.approve>
export type ApproveReturn = FunctionReturn<typeof functions.approve>

export type AvailableParams = FunctionArguments<typeof functions.available>
export type AvailableReturn = FunctionReturn<typeof functions.available>

export type BalanceOfParams = FunctionArguments<typeof functions.balanceOf>
export type BalanceOfReturn = FunctionReturn<typeof functions.balanceOf>

export type BaseParams = FunctionArguments<typeof functions.base>
export type BaseReturn = FunctionReturn<typeof functions.base>

export type BaseURIParams = FunctionArguments<typeof functions.baseURI>
export type BaseURIReturn = FunctionReturn<typeof functions.baseURI>

export type ControllersParams = FunctionArguments<typeof functions.controllers>
export type ControllersReturn = FunctionReturn<typeof functions.controllers>

export type DomainParams = FunctionArguments<typeof functions.domain>
export type DomainReturn = FunctionReturn<typeof functions.domain>

export type DomainNameHashParams = FunctionArguments<typeof functions.domainNameHash>
export type DomainNameHashReturn = FunctionReturn<typeof functions.domainNameHash>

export type ForwardToResolverParams = FunctionArguments<typeof functions.forwardToResolver>
export type ForwardToResolverReturn = FunctionReturn<typeof functions.forwardToResolver>

export type GetApprovedParams = FunctionArguments<typeof functions.getApproved>
export type GetApprovedReturn = FunctionReturn<typeof functions.getApproved>

export type GetOwnerOfParams = FunctionArguments<typeof functions.getOwnerOf>
export type GetOwnerOfReturn = FunctionReturn<typeof functions.getOwnerOf>

export type GetTokenIdParams = FunctionArguments<typeof functions.getTokenId>
export type GetTokenIdReturn = FunctionReturn<typeof functions.getTokenId>

export type IsApprovedForAllParams = FunctionArguments<typeof functions.isApprovedForAll>
export type IsApprovedForAllReturn = FunctionReturn<typeof functions.isApprovedForAll>

export type IsOwnerParams = FunctionArguments<typeof functions.isOwner>
export type IsOwnerReturn = FunctionReturn<typeof functions.isOwner>

export type MigrateNamesParams = FunctionArguments<typeof functions.migrateNames>
export type MigrateNamesReturn = FunctionReturn<typeof functions.migrateNames>

export type MigratedParams = FunctionArguments<typeof functions.migrated>
export type MigratedReturn = FunctionReturn<typeof functions.migrated>

export type MigrationFinishedParams = FunctionArguments<typeof functions.migrationFinished>
export type MigrationFinishedReturn = FunctionReturn<typeof functions.migrationFinished>

export type NameParams = FunctionArguments<typeof functions.name>
export type NameReturn = FunctionReturn<typeof functions.name>

export type OnERC721ReceivedParams = FunctionArguments<typeof functions.onERC721Received>
export type OnERC721ReceivedReturn = FunctionReturn<typeof functions.onERC721Received>

export type OwnerParams = FunctionArguments<typeof functions.owner>
export type OwnerReturn = FunctionReturn<typeof functions.owner>

export type OwnerOfParams = FunctionArguments<typeof functions.ownerOf>
export type OwnerOfReturn = FunctionReturn<typeof functions.ownerOf>

export type ReclaimParams_0 = FunctionArguments<typeof functions['reclaim(uint256,address)']>
export type ReclaimReturn_0 = FunctionReturn<typeof functions['reclaim(uint256,address)']>

export type ReclaimParams_1 = FunctionArguments<typeof functions['reclaim(uint256)']>
export type ReclaimReturn_1 = FunctionReturn<typeof functions['reclaim(uint256)']>

export type ReclaimDomainParams = FunctionArguments<typeof functions.reclaimDomain>
export type ReclaimDomainReturn = FunctionReturn<typeof functions.reclaimDomain>

export type RegisterParams = FunctionArguments<typeof functions.register>
export type RegisterReturn = FunctionReturn<typeof functions.register>

export type RegistryParams = FunctionArguments<typeof functions.registry>
export type RegistryReturn = FunctionReturn<typeof functions.registry>

export type RemoveControllerParams = FunctionArguments<typeof functions.removeController>
export type RemoveControllerReturn = FunctionReturn<typeof functions.removeController>

export type RenounceOwnershipParams = FunctionArguments<typeof functions.renounceOwnership>
export type RenounceOwnershipReturn = FunctionReturn<typeof functions.renounceOwnership>

export type SafeTransferFromParams_0 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256)']>
export type SafeTransferFromReturn_0 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256)']>

export type SafeTransferFromParams_1 = FunctionArguments<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>
export type SafeTransferFromReturn_1 = FunctionReturn<typeof functions['safeTransferFrom(address,address,uint256,bytes)']>

export type SetApprovalForAllParams = FunctionArguments<typeof functions.setApprovalForAll>
export type SetApprovalForAllReturn = FunctionReturn<typeof functions.setApprovalForAll>

export type SetResolverParams = FunctionArguments<typeof functions.setResolver>
export type SetResolverReturn = FunctionReturn<typeof functions.setResolver>

export type SubdomainsParams = FunctionArguments<typeof functions.subdomains>
export type SubdomainsReturn = FunctionReturn<typeof functions.subdomains>

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

export type TopdomainParams = FunctionArguments<typeof functions.topdomain>
export type TopdomainReturn = FunctionReturn<typeof functions.topdomain>

export type TopdomainNameHashParams = FunctionArguments<typeof functions.topdomainNameHash>
export type TopdomainNameHashReturn = FunctionReturn<typeof functions.topdomainNameHash>

export type TotalSupplyParams = FunctionArguments<typeof functions.totalSupply>
export type TotalSupplyReturn = FunctionReturn<typeof functions.totalSupply>

export type TransferDomainOwnershipParams = FunctionArguments<typeof functions.transferDomainOwnership>
export type TransferDomainOwnershipReturn = FunctionReturn<typeof functions.transferDomainOwnership>

export type TransferFromParams = FunctionArguments<typeof functions.transferFrom>
export type TransferFromReturn = FunctionReturn<typeof functions.transferFrom>

export type TransferOwnershipParams = FunctionArguments<typeof functions.transferOwnership>
export type TransferOwnershipReturn = FunctionReturn<typeof functions.transferOwnership>

export type UpdateBaseParams = FunctionArguments<typeof functions.updateBase>
export type UpdateBaseReturn = FunctionReturn<typeof functions.updateBase>

export type UpdateBaseURIParams = FunctionArguments<typeof functions.updateBaseURI>
export type UpdateBaseURIReturn = FunctionReturn<typeof functions.updateBaseURI>

export type UpdateRegistryParams = FunctionArguments<typeof functions.updateRegistry>
export type UpdateRegistryReturn = FunctionReturn<typeof functions.updateRegistry>

