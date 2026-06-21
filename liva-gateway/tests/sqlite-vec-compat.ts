import { platform, arch } from "node:process";
import * as path from "node:path";
import * as fs from "node:fs";

const BASE_PACKAGE_NAME = "sqlite-vec";
const ENTRYPOINT_BASE_NAME = "vec0";
const supportedPlatforms = [
  ["darwin", "x64"],
  ["linux", "x64"],
  ["darwin", "arm64"],
  ["win32", "x64"],
  ["linux", "arm64"]
];

function validPlatform(p: string, a: string) {
  return supportedPlatforms.find(([sp, sa]) => p === sp && a === sa) !== undefined;
}

function extensionSuffix(p: string) {
  if (p === "win32") return "dll";
  if (p === "darwin") return "dylib";
  return "so";
}

function platformPackageName(p: string, a: string) {
  const os = p === "win32" ? "windows" : p;
  return `${BASE_PACKAGE_NAME}-${os}-${a}`;
}

export function getLoadablePath() {
  if (!validPlatform(platform, arch)) {
    throw new Error(
      `Unsupported platform for ${BASE_PACKAGE_NAME}, on a ${platform}-${arch} machine.`
    );
  }
  const packageName = platformPackageName(platform, arch);
  const fileName = `${ENTRYPOINT_BASE_NAME}.${extensionSuffix(platform)}`;
  
  try {
    // @ts-ignore
    return require.resolve(`${packageName}/${fileName}`);
  } catch (err) {
    const rootPath = path.resolve(__dirname, `../../node_modules/${packageName}/${fileName}`);
    if (fs.existsSync(rootPath)) {
      return rootPath;
    }
    const localPath = path.resolve(__dirname, `../node_modules/${packageName}/${fileName}`);
    if (fs.existsSync(localPath)) {
      return localPath;
    }
    throw new Error(`Could not find loadable path for ${packageName}/${fileName}: ${err}`);
  }
}

export function load(db: any) {
  db.loadExtension(getLoadablePath());
}
