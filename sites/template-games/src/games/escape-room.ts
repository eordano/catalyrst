import { TextShape, Transform } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import type { GameDef } from '../harness'

export const escapeRoom: GameDef = {
  id: 'escape-room',
  start(h) {
    const door = h.byName('Locked Door')
    const lever = h.byName('Escape Lever')
    const key = h.byName('Brass Key')
    const hint = h.makeStatusBoard(
      'Find the key.\nPull the lever.\nBeat the clock.',
      Vector3.create(8, 2.9, 13.4)
    )

    let hasKey = false
    let doorOpen = false

    if (key !== null) {
      h.onPointerDown(key, 'Take the key', () => {
        hasKey = true
        Transform.getMutable(key).scale = { x: 0, y: 0, z: 0 }
        TextShape.getMutable(hint).text = 'You have the key.\nNow pull the lever.'
      })
    }

    if (lever !== null) {
      h.onPointerDown(lever, 'Pull the lever', () => {
        if (!hasKey) {
          TextShape.getMutable(hint).text = 'The lever is jammed.\nFind the key first.'
          return
        }
        if (doorOpen || door === null) return
        doorOpen = true
        Transform.getMutable(door).position.y = -3.2
        TextShape.getMutable(hint).text = 'The door grinds open.\nYou escaped!'
      })
    }
  },
}
