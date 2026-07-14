import { createRequire } from 'node:module'
import fs from 'node:fs'
import path from 'node:path'

const workdir = path.resolve(process.argv[2] ?? '.')
const require = createRequire(path.join(workdir, 'package.json'))

// @dcl/sdk-commands is resolved from the target scene's own node_modules (no
// @types available at this dynamic resolution root); only the slice of its
// `components`/`getAllComposites` contract this driver actually exercises is
// captured here.
interface SdkCommandsFsComponent {
  readFile(p: string): Promise<Buffer>
  writeFile(p: string, data: unknown): Promise<void>
}
interface SdkCommandsLoggerComponent {
  log(...args: unknown[]): void
  info(...args: unknown[]): void
  warn(...args: unknown[]): void
  error(...args: unknown[]): void
  debug(...args: unknown[]): void
}
interface SdkCommandsComponents {
  fs: SdkCommandsFsComponent
  logger: SdkCommandsLoggerComponent
}
interface GetAllCompositesResult {
  withErrors: boolean
  compositeLines: string[]
}
type GetAllComposites = (components: SdkCommandsComponents, dir: string) => Promise<GetAllCompositesResult>

const { getAllComposites } = require('@dcl/sdk-commands/dist/logic/composite.js') as {
  getAllComposites: GetAllComposites
}

const logToStderr = (...args: unknown[]): void => void console.error(...args)
const components: SdkCommandsComponents = {
  fs: {
    readFile: (p: string) => fs.promises.readFile(p),
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
