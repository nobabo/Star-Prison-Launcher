import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PACKAGE_PATH = path.join(ROOT_DIR, "package.json");
const CARGO_TOML_PATH = path.join(ROOT_DIR, "src-tauri", "Cargo.toml");
const TAURI_CONFIG_PATH = path.join(ROOT_DIR, "src-tauri", "tauri.conf.json");

function fail(message) {
  throw new Error(message);
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function writeTextIfChanged(filePath, content) {
  const current = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : null;
  if (current === content) {
    return false;
  }

  fs.writeFileSync(filePath, content, "utf8");
  return true;
}

export function readPackageVersion() {
  const packageJson = readJson(PACKAGE_PATH);
  const version = String(packageJson.version ?? "").trim();
  if (!version) {
    fail("package.json version must be set.");
  }
  return version;
}

function syncCargoToml(version) {
  const current = fs.readFileSync(CARGO_TOML_PATH, "utf8");
  const next = current.replace(
    /(^\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
    (_match, prefix) => `${prefix}"${version}"`,
  );

  if (next === current && !current.includes(`version = "${version}"`)) {
    fail("Could not update src-tauri/Cargo.toml package version.");
  }

  return writeTextIfChanged(CARGO_TOML_PATH, next);
}

function syncTauriConfig(version) {
  const current = fs.readFileSync(TAURI_CONFIG_PATH, "utf8");
  const next = current.replace(
    /(^\s*"version"\s*:\s*)"[^"]+"/m,
    (_match, prefix) => `${prefix}"${version}"`,
  );

  if (next === current && !current.includes(`"version": "${version}"`)) {
    fail("Could not update src-tauri/tauri.conf.json version.");
  }

  return writeTextIfChanged(TAURI_CONFIG_PATH, next);
}

export function syncPackageVersion({ quiet = false } = {}) {
  const version = readPackageVersion();
  const changedFiles = [];

  for (const [label, changed] of [
    ["src-tauri/Cargo.toml", syncCargoToml(version)],
    ["src-tauri/tauri.conf.json", syncTauriConfig(version)],
  ]) {
    if (changed) {
      changedFiles.push(label);
    }
  }

  if (!quiet) {
    if (changedFiles.length === 0) {
      console.log(`Version metadata is already synced to package.json ${version}.`);
    } else {
      console.log(`Synced version metadata to package.json ${version}:`);
      for (const filePath of changedFiles) {
        console.log(`- ${filePath}`);
      }
    }
  }

  return {
    version,
    changedFiles,
  };
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    syncPackageVersion();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
