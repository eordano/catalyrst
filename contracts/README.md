# Landiler contracts

On-chain pieces of the Landiler marketplace. Not part of the cargo build - no Foundry/Hardhat toolchain
in this workspace and nothing in the Rust crates compiles or links these. Plain Solidity sources,
deployed once (manually), then referenced by address via env (`LANDILER_ESCROW_ADDRESS`).

## `LandilerEscrow.sol`

15-day reclaimable custody escrow for wearables/emotes bought through the fiat -> Credits -> NFT flow.
Modeled on [`decentraland/rentals-contract`](https://github.com/decentraland/rentals-contract)
(`contracts/Rentals.sol`): custody via `onERC721Received` + `safeTransferFrom`, a per-token timelock
(`unlockAt`), and a transfer-out (`release`/`reclaim`). Wearables/emotes are plain ERC-721s with no
per-token update operator (only LAND has `setUpdateOperator`), so the transfer-lock must be custody:
the escrow physically holds the token for the return window.

- `onERC721Received(.., tokenId, data=abi.encode(buyer))` - records `{buyer, unlockAt = now + lockDuration}` (default 15 days) and accepts custody. The broker delivers each bought NFT via `safeTransferFrom` with the buyer address as the 32-byte `data`.
- `reclaim(collection, tokenId)` - operator-only, BEFORE `unlockAt`: the buyer was refunded inside the window; the asset is retained by Landiler (sent to the operator) and re-sold.
- `release(collection, tokenId, buyer)` - AT/AFTER `unlockAt`: transfers the token to the recorded buyer, ending the lease so the NFT becomes portable network-wide. Permissionless (destination fixed to the recorded buyer) so a keeper can drive it; `buyer` must match the recorded buyer.

Off-chain, `marketplace.usage_grants` (created by `crates/catalyrst-market/migrations/0007_usage_grants.sql`)
overlays the in-escrow item as owned/leased across all six ownership-resolution sites, so it renders on
the avatar and in the backpack during the window. The chain enforces the lock; the overlay is purely UX.

### Deployment notes

- Deploy ONCE to Polygon (chainId 137). Constructor `(owner, operator)`: `owner` is a cold key (sets operator + lock duration), `operator` is the hot Landiler relayer/treasury that funds buys and reclaims inside the window.
- Set `LANDILER_ESCROW_ADDRESS` (consumed by `catalyrst-credits` and `catalyrst-economy`) to the deployed address.
- Imports assume OpenZeppelin Contracts v5 (`Ownable(address)` constructor).
- Polygon MANA ERC-20 (payment token, for reference): `0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4`.
