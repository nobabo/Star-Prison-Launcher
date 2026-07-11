import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import yazl from "yazl";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DISTRIBUTION_PATH = path.join(ROOT_DIR, "config", "distribution.json");
const FILES_DIR = path.join(ROOT_DIR, ".files");
const OUTPUT_DIR = path.join(ROOT_DIR, ".tmp", "release-archives");
const REPOSITORY = "nobabo/Star-Prison-Launcher";
const MAX_REDIRECTS = 8;
const VERIFY_RETRIES = 12;
const VERIFY_RETRY_DELAY_MS = 10_000;
const FIXED_MTIME = new Date("2026-01-01T00:00:00.000Z");

const ARCHIVES = [
  {
    archiveKey: "mods",
    sourceDir: path.join(FILES_DIR, "mods"),
    sourceArchive: path.join(FILES_DIR, "mods.zip"),
  },
  {
    archiveKey: "config",
    sourceDir: path.join(FILES_DIR, "config"),
    sourceArchive: path.join(FILES_DIR, "config.zip"),
  },
  {
    archiveKey: "shaderpacks",
    sourceDir: path.join(FILES_DIR, "shaders"),
    sourceArchive: path.join(FILES_DIR, "shaders.zip"),
  },
];

const shouldCommit = process.argv.includes("--commit");

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
    fail(result.error.message);
  }
  if (result.status !== 0) {
    fail(`${command} failed with exit code ${result.status}`);
  }
}

function readDistribution() {
  return JSON.parse(fs.readFileSync(DISTRIBUTION_PATH, "utf8"));
}

function writeDistribution(distribution) {
  fs.writeFileSync(DISTRIBUTION_PATH, `${JSON.stringify(distribution, null, 2)}\n`, "utf8");
}

function stableRelativePath(rootDir, filePath) {
  return path.relative(rootDir, filePath).split(path.sep).join("/");
}

function listFiles(rootDir) {
  const result = [];

  function visit(directory) {
    const entries = fs.readdirSync(directory, { withFileTypes: true })
      .sort((left, right) => left.name.localeCompare(right.name, "en"));

    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);

      if (entry.isDirectory()) {
        visit(entryPath);
        continue;
      }

      if (entry.isFile()) {
        result.push(entryPath);
      }
    }
  }

  visit(rootDir);
  return result;
}

function createZip(sourceDir, outputPath) {
  if (!fs.existsSync(sourceDir)) {
    fail(`Source directory was not found: ${sourceDir}`);
  }

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.rmSync(outputPath, { force: true });

  const files = listFiles(sourceDir);
  if (files.length === 0) {
    fail(`Source directory has no files: ${sourceDir}`);
  }

  const zipfile = new yazl.ZipFile();
  const outputStream = fs.createWriteStream(outputPath);
  const closePromise = new Promise((resolve, reject) => {
    outputStream.on("close", resolve);
    outputStream.on("error", reject);
    zipfile.outputStream.on("error", reject);
  });

  zipfile.outputStream.pipe(outputStream);

  for (const filePath of files) {
    zipfile.addFile(filePath, stableRelativePath(sourceDir, filePath), {
      mtime: FIXED_MTIME,
      mode: 0o100644,
      compressionLevel: 9,
      forceDosTimestamp: true,
    });
  }

  zipfile.end();
  return closePromise;
}

function digestFile(filePath) {
  const hash = crypto.createHash("sha256");
  const data = fs.readFileSync(filePath);
  hash.update(data);

  return {
    sha256: hash.digest("hex"),
    size: data.length,
  };
}

function request(url, redirectsLeft = MAX_REDIRECTS) {
  return new Promise((resolve, reject) => {
    const parsedUrl = new URL(url);
    const client = parsedUrl.protocol === "http:" ? http : https;

    const req = client.get(parsedUrl, {
      headers: {
        "Cache-Control": "no-cache",
        "Pragma": "no-cache",
        "User-Agent": "star-prison-release-archive-sync",
      },
    }, response => {
      const statusCode = response.statusCode ?? 0;
      const location = response.headers.location;

      if ([301, 302, 303, 307, 308].includes(statusCode) && location != null) {
        response.resume();

        if (redirectsLeft <= 0) {
          reject(new Error(`Too many redirects for ${url}`));
          return;
        }

        resolve(request(new URL(location, parsedUrl).href, redirectsLeft - 1));
        return;
      }

      resolve(response);
    });

    req.on("error", reject);
  });
}

async function digestUrl(url) {
  const response = await request(url);
  const statusCode = response.statusCode ?? 0;

  if (statusCode < 200 || statusCode >= 300) {
    response.resume();
    throw new Error(`HTTP ${statusCode}`);
  }

  const hash = crypto.createHash("sha256");
  let size = 0;

  for await (const chunk of response) {
    size += chunk.length;
    hash.update(chunk);
  }

  return {
    sha256: hash.digest("hex"),
    size,
  };
}

function releaseTag(archive) {
  const version = archive.version;
  if (typeof version === "string" && version.length > 0) {
    return version;
  }

  const match = String(archive.url ?? "").match(/\/releases\/download\/([^/]+)\//);
  if (match) {
    return decodeURIComponent(match[1]);
  }

  fail(`Could not determine release tag for ${archive.url}`);
}

function assetName(archive) {
  const url = new URL(archive.url);
  const name = decodeURIComponent(path.posix.basename(url.pathname));
  if (!name || name === "/") {
    fail(`Could not determine release asset name for ${archive.url}`);
  }
  return name;
}

function cacheBustedUrl(archive, sha256) {
  const url = new URL(archive.url);
  url.searchParams.set("sha", sha256);
  return url.href;
}

function updateArchiveMetadata(archive, digest) {
  let changed = false;
  const url = cacheBustedUrl(archive, digest.sha256);

  if (archive.url !== url) {
    archive.url = url;
    changed = true;
  }

  if (archive.size !== digest.size) {
    archive.size = digest.size;
    changed = true;
  }

  if (String(archive.sha256).toLowerCase() !== digest.sha256) {
    archive.sha256 = digest.sha256;
    changed = true;
  }

  return changed;
}

function commitDistribution() {
  run("git", ["add", "config/distribution.json"]);

  const diffResult = spawnSync("git", ["diff", "--cached", "--quiet", "--", "config/distribution.json"], {
    cwd: ROOT_DIR,
    windowsHide: false,
    shell: false,
  });

  if (diffResult.status === 0) {
    console.log("No distribution metadata changes to commit.");
    return;
  }

  run("git", ["commit", "-m", "Update release archive metadata"]);
}

function sleep(ms) {
  return new Promise(resolve => {
    setTimeout(resolve, ms);
  });
}

async function verifyDistributionWithRetry() {
  for (let attempt = 1; attempt <= VERIFY_RETRIES; attempt += 1) {
    const result = spawnSync("node", [".github/scripts/verify-distribution-assets.mjs"], {
      cwd: ROOT_DIR,
      stdio: "inherit",
      windowsHide: false,
      shell: false,
    });

    if (result.error) {
      fail(result.error.message);
    }

    if (result.status === 0) {
      return;
    }

    if (attempt === VERIFY_RETRIES) {
      fail(`Release archive verification failed after ${VERIFY_RETRIES} attempts.`);
    }

    console.log(`Release asset download did not reflect the upload yet; retrying in ${VERIFY_RETRY_DELAY_MS / 1000}s...`);
    await sleep(VERIFY_RETRY_DELAY_MS);
  }
}

async function syncArchive(distribution, spec) {
  const archive = distribution.channels?.stable?.releaseArchives?.[spec.archiveKey];
  if (!archive) {
    fail(`Missing stable release archive: ${spec.archiveKey}`);
  }

  const outputPath = path.join(OUTPUT_DIR, assetName(archive));
  if (fs.existsSync(spec.sourceArchive)) {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.copyFileSync(spec.sourceArchive, outputPath);
  } else {
    await createZip(spec.sourceDir, outputPath);
  }

  const localDigest = digestFile(outputPath);
  let remoteDigest = null;

  try {
    remoteDigest = await digestUrl(archive.url);
  } catch (error) {
    console.warn(`${spec.archiveKey}: remote digest failed (${error.message}); upload will be attempted.`);
  }

  const remoteMatches = remoteDigest?.sha256 === localDigest.sha256;
  if (remoteMatches) {
    console.log(`${spec.archiveKey}: remote asset already matches ${localDigest.sha256}`);
  } else {
    console.log(`${spec.archiveKey}: uploading ${assetName(archive)} to ${releaseTag(archive)}...`);
    run("gh", [
      "release",
      "upload",
      releaseTag(archive),
      outputPath,
      "--repo",
      REPOSITORY,
      "--clobber",
    ]);
  }

  const metadataChanged = updateArchiveMetadata(archive, localDigest);
  console.log(`${spec.archiveKey}: ${localDigest.size} bytes, ${localDigest.sha256}`);
  return metadataChanged;
}

const distribution = readDistribution();
let changed = false;

for (const spec of ARCHIVES) {
  changed = (await syncArchive(distribution, spec)) || changed;
}

if (changed) {
  writeDistribution(distribution);
  await verifyDistributionWithRetry();

  if (shouldCommit) {
    commitDistribution();
  }
} else {
  console.log("All release archives already match config/distribution.json.");
}

const stableVersion = distribution.channels?.stable?.version;
if (typeof stableVersion === "string" && stableVersion.length > 0) {
  run("gh", ["release", "upload", stableVersion, DISTRIBUTION_PATH, "--repo", REPOSITORY, "--clobber"]);
}
