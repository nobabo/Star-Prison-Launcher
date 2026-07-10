import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";

const DISTRIBUTION_PATH = new URL("../../config/distribution.json", import.meta.url);
const MAX_REDIRECTS = 8;

function fail(message) {
  console.error(message);
  process.exitCode = 1;
}

function request(url, redirectsLeft = MAX_REDIRECTS) {
  return new Promise((resolve, reject) => {
    const parsedUrl = new URL(url);
    const client = parsedUrl.protocol === "http:" ? http : https;

    const req = client.get(parsedUrl, {
      headers: {
        "Cache-Control": "no-cache",
        "Pragma": "no-cache",
        "User-Agent": "star-prison-distribution-verifier",
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

function releaseArchives(distribution) {
  return Object.entries(distribution.channels ?? {})
    .flatMap(([channelName, channel]) => {
      const archives = channel?.releaseArchives ?? {};
      return Object.entries(archives).map(([archiveName, archive]) => ({
        archive,
        archiveName,
        channelName,
      }));
    });
}

const distribution = JSON.parse(fs.readFileSync(DISTRIBUTION_PATH, "utf8"));
const archives = releaseArchives(distribution);

if (archives.length === 0) {
  fail("No releaseArchives entries found in config/distribution.json");
}

for (const { archive, archiveName, channelName } of archives) {
  const label = `${channelName}.${archiveName}`;

  if (typeof archive?.url !== "string" || archive.url.length === 0) {
    fail(`${label}: missing url`);
    continue;
  }

  if (!Number.isSafeInteger(archive.size) || archive.size <= 0) {
    fail(`${label}: invalid size`);
    continue;
  }

  if (typeof archive.sha256 !== "string" || !/^[a-f0-9]{64}$/i.test(archive.sha256)) {
    fail(`${label}: invalid sha256`);
    continue;
  }

  try {
    const actual = await digestUrl(archive.url);
    const sizeMatches = actual.size === archive.size;
    const hashMatches = actual.sha256.toLowerCase() === archive.sha256.toLowerCase();

    if (!sizeMatches || !hashMatches) {
      fail([
        `${label}: asset metadata mismatch`,
        `  url: ${archive.url}`,
        `  expected size: ${archive.size}`,
        `  actual size:   ${actual.size}`,
        `  expected sha:  ${archive.sha256}`,
        `  actual sha:    ${actual.sha256}`,
      ].join("\n"));
      continue;
    }

    console.log(`${label}: ok (${actual.size} bytes, ${actual.sha256})`);
  } catch (error) {
    fail(`${label}: download/verification failed (${archive.url}): ${error.message}`);
  }
}
