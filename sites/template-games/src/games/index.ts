import type { GameDef } from '../harness'
import { castaway2048 } from './castaway-2048'
import { escapeRoom } from './escape-room'
import { memoryGame } from './memory-game'
import { nftArtWall } from './nft-art-wall'
import { towerDefense } from './tower-defense'

export const GAMES: Record<string, GameDef> = {
  'tower-defense': towerDefense,
  'nft-art-wall': nftArtWall,
  'escape-room': escapeRoom,
  'memory-game': memoryGame,
  'castaway-2048': castaway2048,
}
