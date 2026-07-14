import { Material, MeshCollider, TextShape, Transform, type Entity } from '@dcl/sdk/ecs'
import { Color4, Vector3 } from '@dcl/sdk/math'
import type { GameDef } from '../harness'

const TILES = ['Tile 2', 'Tile 4', 'Tile 8', 'Tile 16']
const WIN = 2048

function tileColor(value: number): Color4 {
  const step = Math.min(Math.log2(value), 11) / 11
  return Color4.create(0.93, 0.89 - 0.5 * step, 0.85 - 0.7 * step, 1)
}

export const castaway2048: GameDef = {
  id: 'castaway-2048',
  start(h) {
    const values = new Map<string, number>()
    const board = h.makeStatusBoard(
      'CASTAWAY 2048\nMerge tiles, open the chest',
      Vector3.create(8, 3.2, 11.8)
    )
    const labels = new Map<string, Entity>()

    TILES.forEach((tileName) => {
      const start = parseInt(tileName.split(' ')[1], 10) || 2
      values.set(tileName, start)
      const tile = h.byName(tileName)
      if (tile === null) return
      const pos = Transform.get(tile).position
      labels.set(
        tileName,
        h.makeStatusBoard(
          String(start),
          Vector3.create(pos.x, pos.y + 0.9, pos.z),
          tileName + ' Label'
        )
      )
      MeshCollider.setBox(tile)
      h.onPointerDown(tile, 'Merge', () => {
        const next = (values.get(tileName) ?? 2) * 2
        values.set(tileName, next)
        Material.setPbrMaterial(tile, { albedoColor: tileColor(next) })
        const label = labels.get(tileName)
        if (label !== undefined) TextShape.getMutable(label).text = String(next)
        if (next >= WIN) {
          TextShape.getMutable(board).text =
            'CASTAWAY 2048\nYou made ' + next + ' \u{2014} open the chest!'
        }
      })
    })
  },
}
