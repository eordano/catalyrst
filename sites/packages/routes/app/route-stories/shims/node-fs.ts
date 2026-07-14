function serverOnly(name: string): never {
  throw new Error(`node:fs.${name} is server-only; stub the route loader with fixture data instead`);
}

const deny =
  (name: string) =>
  (..._args: unknown[]): never =>
    serverOnly(name);

export const readFileSync = deny("readFileSync");
export const writeFileSync = deny("writeFileSync");
export const readdirSync = deny("readdirSync");
export const statSync = deny("statSync");
export const lstatSync = deny("lstatSync");
export const mkdirSync = deny("mkdirSync");
export const rmSync = deny("rmSync");
export const unlinkSync = deny("unlinkSync");
export const copyFileSync = deny("copyFileSync");
export const realpathSync = deny("realpathSync");
export const createReadStream = deny("createReadStream");
export const createWriteStream = deny("createWriteStream");
export const watch = deny("watch");
export const watchFile = deny("watchFile");
export const readFile = deny("readFile");
export const writeFile = deny("writeFile");
export const readdir = deny("readdir");
export const stat = deny("stat");
export const mkdir = deny("mkdir");
export const rm = deny("rm");
export const unlink = deny("unlink");
export const access = deny("access");
export function existsSync(): boolean {
  return false;
}
export const constants = { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 };
export const promises = {
  readFile: deny("promises.readFile"),
  writeFile: deny("promises.writeFile"),
  readdir: deny("promises.readdir"),
  stat: deny("promises.stat"),
  lstat: deny("promises.lstat"),
  mkdir: deny("promises.mkdir"),
  rm: deny("promises.rm"),
  unlink: deny("promises.unlink"),
  access: deny("promises.access"),
  copyFile: deny("promises.copyFile"),
  realpath: deny("promises.realpath"),
};

export default {
  readFileSync,
  writeFileSync,
  readdirSync,
  statSync,
  lstatSync,
  mkdirSync,
  rmSync,
  unlinkSync,
  copyFileSync,
  realpathSync,
  createReadStream,
  createWriteStream,
  watch,
  watchFile,
  readFile,
  writeFile,
  readdir,
  stat,
  mkdir,
  rm,
  unlink,
  access,
  existsSync,
  constants,
  promises,
};
