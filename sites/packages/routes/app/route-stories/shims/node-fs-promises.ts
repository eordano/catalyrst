import { promises } from "./node-fs";

export const { readFile, writeFile, readdir, stat, lstat, mkdir, rm, unlink, access, copyFile, realpath } = promises;
export default promises;
