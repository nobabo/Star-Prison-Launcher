import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DISTRIBUTION_PATH = path.join(ROOT_DIR, "config", "distribution.json");
const SERVER_MANIFEST_PATH = path.join(ROOT_DIR, "config", "server.manifest.json");
const FILES_DIR = path.join(ROOT_DIR, ".files");
const OUTPUT_DIR = path.join(ROOT_DIR, ".tmp", "release-archives");
const OUTPUT_PATH = path.join(OUTPUT_DIR, "modpack-manifest.json");
const STAGED_ASSETS_DIR = path.join(OUTPUT_DIR, "release-assets");
const REPOSITORY = "nobabo/Star-Prison-Launcher";
const BASE_RELEASE_URL = "https://github.com/nobabo/Star-Prison-Launcher/releases/download";
const shouldUpload = process.argv.includes("--upload");
const shouldCommit = process.argv.includes("--commit");

function fail(message) { throw new Error(message); }
function readJson(filePath) { return JSON.parse(fs.readFileSync(filePath, "utf8")); }
function writeJson(filePath, value, compact) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, compact ? JSON.stringify(value) : JSON.stringify(value, null, 2) + "\n", "utf8");
}
function digestFile(filePath) {
  const data = fs.readFileSync(filePath);
  return { size: data.length, sha256: crypto.createHash("sha256").update(data).digest("hex") };
}
function listFiles(rootDir) {
  const files = [];
  function visit(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name, "en"))) {
      const filePath = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(filePath);
      else if (entry.isFile()) files.push(filePath);
    }
  }
  if (!fs.existsSync(rootDir)) fail("Missing modpack source directory: " + rootDir);
  visit(rootDir);
  return files;
}
function encodedAssetUrl(version, assetName, sha256) {
  const url = new URL(BASE_RELEASE_URL + "/" + encodeURIComponent(version) + "/" + encodeURIComponent(assetName));
  url.searchParams.set("sha", sha256);
  return url.href;
}
function sourceSpecs(version) {
  return [
    { root: path.join(FILES_DIR, "mods"), kind: "mod", prefix: "mod" },
    { root: path.join(FILES_DIR, "config"), kind: "config-seed", prefix: "config" },
    { root: path.join(FILES_DIR, "shaders"), kind: "shaderpack", prefix: "shaderpack" },
  ].flatMap(spec => listFiles(spec.root).map(filePath => {
    const relativePath = path.relative(spec.root, filePath).split(path.sep).join("/");
    const assetName = spec.prefix + "__" + relativePath.replaceAll("/", "__");
    const digest = digestFile(filePath);
    return {
      sourcePath: filePath,
      assetName,
      path: (spec.kind === "mod" ? "mods" : spec.kind === "config-seed" ? "config" : "shaderpacks") + "/" + relativePath,
      kind: spec.kind,
      url: encodedAssetUrl(version, assetName, digest.sha256),
      size: digest.size,
      sha256: digest.sha256,
      required: true,
    };
  }));
}
function run(command, args) {
  const result = spawnSync(command, args, { cwd: ROOT_DIR, stdio: "inherit", windowsHide: false, shell: false });
  if (result.error) fail(result.error.message);
  if (result.status !== 0) fail(command + " failed with exit code " + result.status);
}
function uploadAssets(version, entries) {
  fs.rmSync(STAGED_ASSETS_DIR, { recursive: true, force: true });
  fs.mkdirSync(STAGED_ASSETS_DIR, { recursive: true });
  const files = entries.map(entry => {
    const stagedPath = path.join(STAGED_ASSETS_DIR, entry.assetName);
    fs.copyFileSync(entry.sourcePath, stagedPath);
    return stagedPath;
  });
  for (let index = 0; index < files.length; index += 20) {
    run("gh", ["release", "upload", version, ...files.slice(index, index + 20), "--repo", REPOSITORY, "--clobber"]);
  }
  run("gh", ["release", "upload", version, OUTPUT_PATH, "--repo", REPOSITORY, "--clobber"]);
  run("gh", ["release", "upload", version, DISTRIBUTION_PATH, "--repo", REPOSITORY, "--clobber"]);
}
function commitDistribution() {
  run("git", ["add", "config/distribution.json", ".github/scripts/sync-modpack-manifest.mjs", "package.json"]);
  run("git", ["commit", "-m", "Add compact modpack manifest distribution"]);
}

try {
  const distribution = readJson(DISTRIBUTION_PATH);
  const stable = distribution.channels && distribution.channels.stable;
  const serverManifest = readJson(SERVER_MANIFEST_PATH);
  if (!stable || typeof stable !== "object") fail("Missing stable distribution channel.");
  const version = String(stable.version || distribution.launcherVersion || "").trim();
  if (!version) fail("Stable distribution version is required.");
  const entries = sourceSpecs(version);
  const manifest = {
    schemaVersion: 1,
    id: "star-prison",
    version,
    minecraftVersion: serverManifest.minecraftVersion,
    loader: "fabric",
    files: entries.map(({ sourcePath: _sourcePath, assetName: _assetName, ...entry }) => entry),
  };
  writeJson(OUTPUT_PATH, manifest, true);
  const manifestDigest = digestFile(OUTPUT_PATH);
  stable.modpackManifest = {
    version,
    fileName: "modpack-manifest.json",
    url: encodedAssetUrl(version, "modpack-manifest.json", manifestDigest.sha256),
    size: manifestDigest.size,
    sha256: manifestDigest.sha256,
  };
  writeJson(DISTRIBUTION_PATH, distribution, false);
  console.log("Generated " + OUTPUT_PATH + ": " + entries.length + " files, " + manifestDigest.size + " bytes, " + manifestDigest.sha256);
  if (shouldUpload) uploadAssets(version, entries);
  if (shouldCommit) commitDistribution();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
