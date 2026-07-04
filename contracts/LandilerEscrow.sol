// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/IERC721.sol";
import "@openzeppelin/contracts/token/ERC721/IERC721Receiver.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title LandilerEscrow
/// @notice 15-day reclaimable custody escrow for Decentraland wearables/emotes
///         bought through the Landiler fiat -> Credits -> NFT marketplace.
///
/// Decentraland wearables/emotes are plain ERC-721s with no per-token update
/// operator (unlike LAND), so a transfer-lock cannot be enforced by a flag on
/// the token — the only way to make an item non-transferable for the return
/// window is to hold it in custody. This contract is that custody, modeled on
/// `decentraland/rentals-contract`'s `Rentals.sol` (custody via
/// `onERC721Received` + `safeTransferFrom` into the contract, an `endDate`-style
/// timelock per token, and a `claim()` that transfers the asset back out):
///
///   * The broker buys each NFT straight to this contract (primary mint with
///     `beneficiaries:[escrow]`, or a secondary buy forwarded here). On receipt
///     the contract records `{buyer, unlockAt = now + 15 days}` for the token.
///   * BEFORE `unlockAt`, only the operator can `reclaim(...)` the token — used
///     when the buyer is refunded inside the return window; the asset is
///     retained by Landiler (sent to the operator) and re-sold.
///   * AT/AFTER `unlockAt`, `release(...)` transfers the token to the recorded
///     buyer, ending the lease — the buyer then holds the NFT on-chain and it is
///     portable network-wide.
///
/// Off-chain, catalyrst's `marketplace.usage_grants` overlay makes the in-escrow
/// item still render on the buyer's avatar and in their backpack (flagged
/// "leased") across all six ownership-resolution sites during the window. The
/// chain is the source of truth for the lock; the overlay is purely UX.
///
/// @dev Deploy ONCE to Polygon. The operator is the Landiler relayer/treasury.
contract LandilerEscrow is IERC721Receiver, Ownable, ReentrancyGuard {
    /// @notice Per-token custody record. Indexed by (collection, tokenId).
    struct Lease {
        /// @dev The buyer the token is released to at/after `unlockAt`.
        address buyer;
        /// @dev Unix timestamp; reclaim is operator-only before it, release is
        ///      allowed at/after it.
        uint64 unlockAt;
    }

    /// @notice The address allowed to `reclaim` inside the return window and to
    ///         tune `lockDuration`. Distinct from `owner()` so day-to-day
    ///         operations can run from a hot relayer while ownership stays cold.
    address public operator;

    /// @notice The escrow/return window applied to newly received tokens.
    uint64 public lockDuration = 15 days;

    /// @notice collection => tokenId => Lease.
    mapping(address => mapping(uint256 => Lease)) public leases;

    /// @notice Emitted when a token enters custody and its lease is recorded.
    event Leased(address indexed collection, uint256 indexed tokenId, address indexed buyer, uint64 unlockAt);

    /// @notice Emitted when the operator reclaims a token before unlock (refund).
    event Reclaimed(address indexed collection, uint256 indexed tokenId, address indexed to);

    /// @notice Emitted when a token is released to its buyer at/after unlock.
    event Released(address indexed collection, uint256 indexed tokenId, address indexed buyer);

    /// @notice Emitted when the operator is changed.
    event OperatorChanged(address indexed previousOperator, address indexed newOperator);

    /// @notice Emitted when the lock duration is changed (applies to future leases).
    event LockDurationChanged(uint64 previousDuration, uint64 newDuration);

    modifier onlyOperator() {
        require(msg.sender == operator, "LandilerEscrow: NOT_OPERATOR");
        _;
    }

    /// @param _owner    cold owner (can set the operator + lock duration).
    /// @param _operator hot relayer allowed to reclaim within the window.
    constructor(address _owner, address _operator) Ownable(_owner) {
        require(_operator != address(0), "LandilerEscrow: ZERO_OPERATOR");
        operator = _operator;
        emit OperatorChanged(address(0), _operator);
    }

    /// @notice Point the escrow at a new operator (e.g. relayer key rotation).
    function setOperator(address _operator) external onlyOwner {
        require(_operator != address(0), "LandilerEscrow: ZERO_OPERATOR");
        emit OperatorChanged(operator, _operator);
        operator = _operator;
    }

    /// @notice Change the escrow window length. Only affects tokens received
    ///         AFTER the change; in-flight leases keep their recorded `unlockAt`.
    function setLockDuration(uint64 _lockDuration) external onlyOwner {
        require(_lockDuration > 0, "LandilerEscrow: ZERO_DURATION");
        emit LockDurationChanged(lockDuration, _lockDuration);
        lockDuration = _lockDuration;
    }

    /// @notice ERC-721 custody entry point. The buyer's address MUST be supplied
    ///         as the 32-byte ABI-encoded `_data` of the `safeTransferFrom` that
    ///         delivers the token here (the broker encodes it). Records the lease
    ///         and starts the return window.
    /// @dev Returns the ERC721Receiver magic value so the transfer succeeds.
    function onERC721Received(
        address, /* operator (the caller of safeTransferFrom) */
        address, /* from */
        uint256 _tokenId,
        bytes calldata _data
    ) external override returns (bytes4) {
        // msg.sender is the ERC-721 collection contract performing the callback.
        address collection = msg.sender;
        require(_data.length == 32, "LandilerEscrow: BAD_BUYER_DATA");
        address buyer = abi.decode(_data, (address));
        require(buyer != address(0), "LandilerEscrow: ZERO_BUYER");
        require(leases[collection][_tokenId].buyer == address(0), "LandilerEscrow: ALREADY_LEASED");

        uint64 unlockAt = uint64(block.timestamp) + lockDuration;
        leases[collection][_tokenId] = Lease({buyer: buyer, unlockAt: unlockAt});

        emit Leased(collection, _tokenId, buyer, unlockAt);
        return IERC721Receiver.onERC721Received.selector;
    }

    /// @notice Reclaim a token still inside its return window. Operator-only —
    ///         used when the buyer is refunded; the asset is retained by Landiler
    ///         (sent to the operator) for re-sale. Reverts at/after `unlockAt`.
    function reclaim(address _collection, uint256 _tokenId) external nonReentrant onlyOperator {
        Lease memory lease = leases[_collection][_tokenId];
        require(lease.buyer != address(0), "LandilerEscrow: NOT_LEASED");
        require(block.timestamp < lease.unlockAt, "LandilerEscrow: WINDOW_CLOSED");

        delete leases[_collection][_tokenId];
        IERC721(_collection).safeTransferFrom(address(this), operator, _tokenId);

        emit Reclaimed(_collection, _tokenId, operator);
    }

    /// @notice Release a token to its recorded buyer at/after `unlockAt`. Callable
    ///         by anyone (the destination is fixed to the recorded buyer, so this
    ///         can be driven by a permissionless keeper). `_buyer` must match the
    ///         recorded buyer — a guard against passing a stale/wrong address.
    function release(address _collection, uint256 _tokenId, address _buyer) external nonReentrant {
        Lease memory lease = leases[_collection][_tokenId];
        require(lease.buyer != address(0), "LandilerEscrow: NOT_LEASED");
        require(block.timestamp >= lease.unlockAt, "LandilerEscrow: WINDOW_OPEN");
        require(lease.buyer == _buyer, "LandilerEscrow: BUYER_MISMATCH");

        delete leases[_collection][_tokenId];
        IERC721(_collection).safeTransferFrom(address(this), lease.buyer, _tokenId);

        emit Released(_collection, _tokenId, lease.buyer);
    }

    /// @notice Convenience view: the lease for a token (zero buyer = none).
    function leaseOf(address _collection, uint256 _tokenId) external view returns (Lease memory) {
        return leases[_collection][_tokenId];
    }
}
