import { TextShape, Transform } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import type { GameDef } from '../harness'

const CREEPS = ['Creep Spider A', 'Creep Spider B']
const GATE_Z = 14
const GOAL_Z = 3
const SPEED = 0.8

export const towerDefense: GameDef = {
  id: 'tower-defense',
  start(h) {
    const board = h.makeStatusBoard(
      'TOWER DEFENSE\nWave 1 \u{2014} hold the path!',
      Vector3.create(8, 2.8, 13.5)
    )
    let breaches = 0

    for (const creepName of CREEPS) {
      const creep = h.byName(creepName)
      if (creep === null) continue
      h.onPointerDown(creep, 'Repel!', () => {
        Transform.getMutable(creep).position.z = GATE_Z
      })
    }

    h.addSystem((dt) => {
      for (const creepName of CREEPS) {
        const creep = h.byName(creepName)
        if (creep === null) continue
        const t = Transform.getMutable(creep)
        t.position.z -= SPEED * dt
        if (t.position.z <= GOAL_Z) {
          t.position.z = GATE_Z
          breaches += 1
          TextShape.getMutable(board).text =
            'TOWER DEFENSE\nBreaches: ' + breaches + ' \u{2014} defend the path!'
        }
      }
    }, 'march creeps')
  },
}
