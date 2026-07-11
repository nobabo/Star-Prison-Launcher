import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DISTRIBUTION_PATH = path.join(ROOT_DIR, "config", "distribution.json");
const SERVER_MANIFEST_PATH = path.join(ROOT_DIR, "config", "server.manifest.json");
const FILES_DIR = path.join(ROOT_DIR, ".files");
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
    for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name, "en"))) {
      const filePath = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(filePath);
      else if (entry.isFile()) files.push(filePath);
    }
  }
  if (!fs.existsSync(rootDir)) fail("Missing modpack source directory: " + rootDir);
  visit(rootDir);
  return files;
}
function sourceSpecs() {
  return [
    { root: path.join(FILES_DIR, "mods"), kind: "mod" },
    { root: path.join(FILES_DIR, "config"), kind: "config-seed" },
    { root: path.join(FILES_DIR, "shaders"), kind: "shaderpack" },
  ].flatMap(spec => listFiles(spec.root).map(filePath => {
    const relativePath = path.relative(spec.root, filePath).split(path.sep).join("/");
    const entry = {
      path: (spec.kind === "mod" ? "mods" : spec.kind === "config-seed" ? "config" : "shaderpacks") + "/" + relativePath,
      kind: spec.kind,
      required: true,
    };

    if (spec.kind === "config-seed") {
      Object.assign(entry, digestFile(filePath));
    }

    return entry;
  }));
}
function run(command, args) {
  const result = spawnSync(command, args, { cwd: ROOT_DIR, stdio: "inherit", windowsHide: false, shell: false });
  if (result.error) fail(result.error.message);
  if (result.status !== 0) fail(command + " failed with exit code " + result.status);
}
function commitDistribution() {
  run("git", ["add", "config/distribution.json", ".github/scripts/sync-modpack-manifest.mjs", "package.json"]);
  run("git", ["commit", "-m", "Embed compact modpack manifest in distribution"]);
}

try {
  const distribution = readJson(DISTRIBUTION_PATH);
  const stable = distribution.channels && distribution.channels.stable;
  const serverManifest = readJson(SERVER_MANIFEST_PATH);
  if (!stable || typeof stable !== "object") fail("Missing stable distribution channel.");
  const version = String(stable.version || distribution.launcherVersion || "").trim();
  if (!version) fail("Stable distribution version is required.");

  const manifest = {
    schemaVersion: 1,
    id: "star-prison",
    version,
    minecraftVersion: serverManifest.minecraftVersion,
    loader: "fabric",
    files: sourceSpecs(),
  };

  stable.modpackManifest = manifest;
  writeJson(DISTRIBUTION_PATH, distribution, false);
  console.log("Embedded compact modpack manifest in distribution.json: " + manifest.files.length + " files");
  if (shouldCommit) commitDistribution();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
