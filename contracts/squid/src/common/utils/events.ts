import { Store } from '@subsquid/typeorm-store'
import { TransferReceivedEvent, Events } from '@dcl/schemas'
import { EntityManager } from 'typeorm'
import eventPublisher from './event_publisher'

// A pending TRANSFER_RECEIVED ("gift") notification collected while processing a
// batch. Whether a transfer is an actual gift (vs a marketplace purchase) can
// only be decided once the whole batch is processed, because within a single
// transaction the ERC721 Transfer log is handled before the marketplace event
// (OrderSuccessful / BidAccepted / Traded) that records the sale. See the
// post-batch reconciliation in the polygon processor.
export type TransferGiftCandidate = {
  nftId: string
  from: string
  to: string
  tokenURI: string | null
  // NFT.updatedAt, in seconds (matches the block timestamp of the transfer).
  timestamp: bigint
  txHash: string
}

export async function getLastNotified(store: Store): Promise<bigint | null> {
  const em = (store as unknown as { em: () => EntityManager }).em()
  const result = await em.query(
    "SELECT last_notified FROM public.squids WHERE name = $1",
    ['marketplace']
  )
  if (!result || result.length === 0) {
    return null
  }
  const lastNotified = result[0]?.last_notified
  return lastNotified ? BigInt(lastNotified) : null
}

export async function setLastNotified(store: Store, timestamp: bigint) {
  const em = (store as unknown as { em: () => EntityManager }).em()
  await em.query(
    "UPDATE public.squids SET last_notified = $1 WHERE name = $2",
    [timestamp.toString(), 'marketplace']
  )
}

// Publishes a single gift notification. The timestamp is emitted in milliseconds
// to stay consistent with the rest of the notification pipeline.
export async function publishTransferGift(candidate: TransferGiftCandidate): Promise<void> {
  const event: TransferReceivedEvent = {
    type: Events.Type.BLOCKCHAIN,
    subType: Events.SubType.Blockchain.TRANSFER_RECEIVED,
    key: candidate.nftId,
    timestamp: Number(candidate.timestamp) * 1000,
    metadata: {
      senderAddress: candidate.from,
      receiverAddress: candidate.to,
      tokenUri: candidate.tokenURI ?? undefined,
    },
  }
  await eventPublisher.publishMessage(event)
}
