import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const FILES_DIR = path.join(ROOT_DIR, ".files");
const DISTRIBUTION_PATH = "config/distribution.json";
const POLL_INTERVAL_MS = 2_000;
const RETRY_INTERVAL_MS = 15_000;

const SOURCES = [
  {
    archive: path.join(FILES_DIR, "mods.zip"),
    directory: path.join(FILES_DIR, "mods"),
    name: "mods.zip",
  },
  {
    archive: path.join(FILES_DIR, "config.zip"),
    directory: path.join(FILES_DIR, "config"),
    name: "config.zip",
  },
  {
    archive: path.join(FILES_DIR, "shaders.zip"),
    directory: path.join(FILES_DIR, "shaders"),
    name: "shaders.zip",
  },
];

function fail(message) {
  console.error(message);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT_DIR,
    stdio: "inherit",
    windowsHide: false,
    shell: false,
    ...options,
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit code ${result.status}`);
  }
}

function git(args, options = {}) {
  return spawnSync("git", ["-c", `safe.directory=${ROOT_DIR}`, ...args], {
    cwd: ROOT_DIR,
    windowsHide: false,
    shell: false,
    ...options,
  });
}

function gitOutput(args) {
  const result = git(args, { encoding: "utf8" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`git ${args.join(" ")} failed with exit code ${result.status}`);
  }
  return result.stdout.trim();
}

function listFiles(directory) {
  const result = [];

  function visit(currentDirectory) {
    const entries = fs
      .readdirSync(currentDirectory, { withFileTypes: true })
      .sort((left, right) => left.name.localeCompare(right.name, "en"));

    for (const entry of entries) {
      const entryPath = path.join(currentDirectory, entry.name);
      if (entry.isDirectory()) {
        visit(entryPath);
      } else if (entry.isFile()) {
        result.push(entryPath);
      }
    }
  }

  visit(directory);
  return result;
}

function selectedSource(spec) {
  if (fs.existsSync(spec.archive)) {
    return { files: [spec.archive], root: FILES_DIR };
  }
  if (fs.existsSync(spec.directory)) {
    return { files: listFiles(spec.directory), root: spec.directory };
  }
  fail(`${spec.name} source was not found: ${spec.archive} or ${spec.directory}`);
}

function sourceSnapshot() {
  const hash = crypto.createHash("sha256");

  for (const spec of SOURCES) {
    const source = selectedSource(spec);
    hash.update(`${spec.name}\0`);

    for (const filePath of source.files) {
      hash.update(`${path.relative(source.root, filePath).split(path.sep).join("/")}\0`);
      hash.update(fs.readFileSync(filePath));
      hash.update("\0");
    }
  }

  return hash.digest("hex");
}

function distributionIsClean() {
  const worktree = git(["diff", "--quiet", "--", DISTRIBUTION_PATH]);
  const index = git(["diff", "--cached", "--quiet", "--", DISTRIBUTION_PATH]);
  return worktree.status === 0 && index.status === 0;
}

function syncRelease() {
  if (!distributionIsClean()) {
    throw new Error(
      "config/distribution.json has unrelated changes. Commit or stash them before automatic release syncing.",
    );
  }

  const head = gitOutput(["rev-parse", "HEAD"]);
  const upstream = gitOutput(["rev-parse", "@{upstream}"]);
  if (head !== upstream) {
    throw new Error(
      "The current branch has unpublished commits. Push or reconcile them before automatic release syncing.",
    );
  }

  run("node", [".github/scripts/sync-modpack-manifest.mjs"]);
  run("node", [".github/scripts/sync-release-archives.mjs", "--commit"]);
  run("git", ["-c", `safe.directory=${ROOT_DIR}`, "push"]);
}

run("gh", ["auth", "status"]);

let publishedSnapshot = null;
let nextAttemptAt = 0;
let stopped = false;

console.log("Watching .files release sources. Press Ctrl+C to stop.");

process.on("SIGINT", () => {
  stopped = true;
});
process.on("SIGTERM", () => {
  stopped = true;
});

while (!stopped) {
  const snapshot = sourceSnapshot();
  const now = Date.now();

  if (snapshot !== publishedSnapshot && now >= nextAttemptAt) {
    try {
      console.log(`Release source change detected (${snapshot}).`);
      syncRelease();
      publishedSnapshot = sourceSnapshot();
      nextAttemptAt = 0;
      console.log("Release assets, distribution metadata, commit, and push are synchronized.");
    } catch (error) {
      console.error(`Release sync failed: ${error.message}`);
      nextAttemptAt = Date.now() + RETRY_INTERVAL_MS;
    }
  }

  await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL_MS));
}

console.log("Release asset watcher stopped.");
