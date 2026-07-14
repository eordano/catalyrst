import { Material, MeshCollider } from '@dcl/sdk/ecs'
import { Color3, Color4 } from '@dcl/sdk/math'
import type { GameDef } from '../harness'

const CANVASES = ['Canvas 1', 'Canvas 2', 'Canvas 3']
const PALETTE = [
  Color4.create(0.9, 0.35, 0.25, 1),
  Color4.create(0.25, 0.65, 0.95, 1),
  Color4.create(0.55, 0.9, 0.4, 1),
  Color4.create(0.95, 0.8, 0.2, 1),
  Color4.create(0.75, 0.3, 0.9, 1),
]

export const nftArtWall: GameDef = {
  id: 'nft-art-wall',
  start(h) {
    CANVASES.forEach((canvasName, i) => {
      const canvas = h.byName(canvasName)
      if (canvas === null) return
      let colorIndex = i
      MeshCollider.setPlane(canvas)
      h.onPointerDown(canvas, 'Cycle art', () => {
        colorIndex = (colorIndex + 1) % PALETTE.length
        const c = PALETTE[colorIndex]
        Material.setPbrMaterial(canvas, {
          albedoColor: c,
          emissiveColor: Color3.create(c.r, c.g, c.b),
          emissiveIntensity: 2,
        })
      })
    })
  },
}
