import { Store } from '@subsquid/typeorm-store'
import { TransferReceivedEvent, Events } from '@dcl/schemas'
import { EntityManager } from 'typeorm'
import eventPublisher from './event_publisher'

export type TransferGiftCandidate = {
  nftId: string
  from: string
  to: string
  tokenURI: string | null
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
