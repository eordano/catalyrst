import { createRequire } from 'node:module'
import fs from 'node:fs'
import path from 'node:path'

const workdir = path.resolve(process.argv[2] ?? '.')
const require = createRequire(path.join(workdir, 'package.json'))
const { getAllComposites } = require('@dcl/sdk-commands/dist/logic/composite.js')

const logToStderr = (...args) => console.error(...args)
const components = {
  fs: {
    readFile: (p) => fs.promises.readFile(p),
    writeFile: async () => {}
  },
  logger: {
    log: logToStderr,
    info: logToStderr,
    warn: logToStderr,
    error: logToStderr,
    debug: logToStderr
  }
}

const data = await getAllComposites(components, workdir)
if (data.withErrors) console.error('NOTE: getAllComposites reported withErrors=true')
process.stdout.write(`export const compositeFromLoader = {${data.compositeLines.join(',')}}`)
