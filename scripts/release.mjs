import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createInterface } from "node:readline/promises";
import { stdin as input, stdout as output } from "node:process";

const REPO = "t8y2/dbx";
const PACKAGES_WORKFLOW = "mcp-release.yml";
const APP_PUBLISH_WORKFLOW = "publish-packages.yml";
const PACKAGE_TAG_PREFIX = "packages-v";
const AGENT_TAG_PREFIX = "agents-v";
const APP_TAG_PREFIX = "v";

const args = process.argv.slice(2);
let target = null;
let requestedBump = null;
let dryRun = false;
let yes = false;
let skipFetch = false;

for (const arg of args) {
  switch (arg) {
    case "--":
      break;
    case "--dry-run":
      dryRun = true;
      break;
    case "-y":
    case "--yes":
      yes = true;
      break;
    case "--skip-fetch":
      skipFetch = true;
      break;
    case "-h":
    case "--help":
      printHelp();
      process.exit(0);
      break;
    default: {
      const normalizedTarget = normalizeTarget(arg);
      if (!target && normalizedTarget) {
        target = normalizedTarget;
      } else if (!requestedBump) {
        requestedBump = arg;
      } else {
        fail(`Unexpected argument: ${arg}`);
      }
      break;
    }
  }
}

const repoRoot = run("git", ["rev-parse", "--show-toplevel"]).stdout.trim();
process.chdir(repoRoot);

if (!skipFetch) {
  fetchReleaseTags();
}

if (!target) {
  if (!process.stdin.isTTY) {
    fail("Release target is required in a non-interactive shell. Use packages, agents, or app.");
  }
  target = await promptTarget();
}

if (target === "packages") {
  await releasePackages(requestedBump ?? "patch");
} else if (target === "agents") {
  await releaseAgents(requestedBump ?? "patch");
} else if (target === "app") {
  await publishApp(requestedBump);
} else {
  fail(`Unknown release target: ${target}`);
}

async function releasePackages(bump) {
  const latestVersion = getLatestPackageVersion();
  const releaseVersion = resolveReleaseVersion(bump, latestVersion, PACKAGE_TAG_PREFIX);
  const releaseTag = `${PACKAGE_TAG_PREFIX}${releaseVersion}`;
  const workflowArgs = ["workflow", "run", PACKAGES_WORKFLOW, "--repo", REPO, "-f", `version=${releaseVersion}`];

  console.log(`Release target: Node packages / MCP`);
  console.log(`Current package version: ${latestVersion ?? "none"}`);
  console.log(`New package version: ${releaseVersion}`);
  console.log(`Release tag: ${releaseTag}`);
  console.log(`Workflow: Node Packages Release (${PACKAGES_WORKFLOW})`);
  console.log(`Command: gh ${workflowArgs.join(" ")}`);

  if (dryRun) {
    console.log("Dry run only; workflow was not triggered.");
    return;
  }

  ensureGhReady(PACKAGES_WORKFLOW);
  await confirmOrExit(`Confirm triggering Node Packages Release for ${releaseVersion}? [y/N] `);

  run("gh", workflowArgs, { stdio: "inherit" });
  console.log(`Triggered Node Packages Release for ${releaseVersion}.`);
}

async function releaseAgents(bump) {
  const latest = getLatestAgentTag();
  const releaseTag = resolveAgentTag(bump, latest.tag);

  if (tagExists(releaseTag)) {
    fail(`Tag ${releaseTag} already exists.`);
  }

  console.log(`Release target: Agents`);
  console.log(`Current agent tag: ${latest.tag}${latest.source ? ` (${latest.source})` : ""}`);
  console.log(`New agent tag: ${releaseTag}`);
  console.log(`Workflow: Agents Release (.github/workflows/agents-release.yml)`);
  console.log(`Commands: git tag ${releaseTag} && git push origin ${releaseTag}`);

  if (dryRun) {
    console.log("Dry run only; tag was not created or pushed.");
    return;
  }

  await confirmOrExit(`Confirm creating and pushing tag ${releaseTag}? [y/N] `);

  run("git", ["tag", releaseTag], { stdio: "inherit" });
  run("git", ["push", "origin", releaseTag], { stdio: "inherit" });
  console.log(`Pushed ${releaseTag}; Agents Release will run from the tag push.`);
}

async function publishApp(tagInput) {
  const latest = getLatestAppTag();
  const releaseTag = tagInput ? resolveAppTag(tagInput) : latest.tag;
  const workflowArgs = ["workflow", "run", APP_PUBLISH_WORKFLOW, "--repo", REPO, "-f", `tag=${releaseTag}`];

  console.log(`Release target: App distribution`);
  console.log(`Latest app tag: ${latest.tag}`);
  console.log(`Publish tag: ${releaseTag}`);
  console.log(`Workflow: Publish Packages (${APP_PUBLISH_WORKFLOW})`);
  console.log(`Command: gh ${workflowArgs.join(" ")}`);

  if (dryRun) {
    console.log("Dry run only; workflow was not triggered.");
    return;
  }

  ensureGhReady(APP_PUBLISH_WORKFLOW);
  run("gh", ["release", "view", releaseTag, "--repo", REPO], { stdio: "inherit" });
  await confirmOrExit(`Confirm publishing app distribution for ${releaseTag}? [y/N] `);

  run("gh", workflowArgs, { stdio: "inherit" });
  console.log(`Triggered Publish Packages for ${releaseTag}.`);
}

async function promptTarget() {
  const answer = await ask(`Select release target:
  1. Node packages / MCP
  2. Agents
  3. App distribution
Choice [1]: `);

  const normalized = answer.trim().toLowerCase();
  if (!normalized || normalized === "1" || normalized === "packages" || normalized === "mcp") return "packages";
  if (normalized === "2" || normalized === "agents" || normalized === "agent") return "agents";
  if (normalized === "3" || normalized === "app" || normalized === "desktop" || normalized === "publish") return "app";
  fail(`Unknown release target: ${answer}`);
}

async function confirmOrExit(message) {
  if (yes) return;
  if (!process.stdin.isTTY) {
    fail("Refusing to trigger release without confirmation in a non-interactive shell. Re-run with --yes if this is intentional.");
  }

  const answer = await ask(message);
  if (!["y", "yes"].includes(answer.trim().toLowerCase())) {
    console.log("Cancelled.");
    process.exit(0);
  }
}

async function ask(question) {
  const rl = createInterface({ input, output });
  const answer = await rl.question(question);
  rl.close();
  return answer;
}

function normalizeTarget(value) {
  if (["package", "packages", "node-packages", "node", "mcp"].includes(value)) return "packages";
  if (["agent", "agents"].includes(value)) return "agents";
  if (["app", "desktop", "publish", "publish-packages", "distribution"].includes(value)) return "app";
  return null;
}

function getLatestPackageVersion() {
  const tag = getLatestSemverTag(PACKAGE_TAG_PREFIX);
  if (tag) return tag.versionText;

  const packageVersions = [
    "packages/node-core/package.json",
    "packages/cli/package.json",
    "packages/mcp-server/package.json",
  ].map((path) => JSON.parse(readFileSync(path, "utf8")).version);

  const uniqueVersions = [...new Set(packageVersions)];
  if (uniqueVersions.length !== 1) {
    fail(`Package versions differ and no ${PACKAGE_TAG_PREFIX} tag was found: ${uniqueVersions.join(", ")}`);
  }

  return uniqueVersions[0];
}

function getLatestAgentTag() {
  const tag = getLatestSemverTag(AGENT_TAG_PREFIX);
  if (tag) return { tag: tag.tag, source: "current repo" };

  const legacyRepo = "../dbx-agents";
  const legacyRepoCheck = spawnSync("git", ["-C", legacyRepo, "rev-parse", "--is-inside-work-tree"], {
    cwd: process.cwd(),
    encoding: "utf8",
    stdio: "pipe",
  });

  if (legacyRepoCheck.status === 0) {
    const legacyTags = run("git", ["-C", legacyRepo, "tag", "--list", "v*"]).stdout
      .split(/\r?\n/)
      .map((legacyTag) => legacyTag.trim())
      .filter(Boolean)
      .map((legacyTag) => ({ legacyTag, version: parseVersion(legacyTag.replace(/^v/, "")) }))
      .filter((entry) => entry.version)
      .sort((a, b) => compareVersions(b.version, a.version));

    if (legacyTags.length > 0) {
      return {
        tag: `${AGENT_TAG_PREFIX}${formatVersion(legacyTags[0].version)}`,
        source: `${legacyRepo} ${legacyTags[0].legacyTag}`,
      };
    }
  }

  return { tag: `${AGENT_TAG_PREFIX}0.0.0`, source: "initial baseline" };
}

function getLatestAppTag() {
  const tag = getLatestSemverTag(APP_TAG_PREFIX);
  if (!tag) {
    fail("No v* app release tag was found.");
  }
  return { tag: tag.tag };
}

function getLatestSemverTag(prefix) {
  const tags = run("git", ["tag", "--list", `${prefix}*`]).stdout
    .split(/\r?\n/)
    .map((tag) => tag.trim())
    .filter(Boolean)
    .map((tag) => ({ tag, version: parseVersion(tag.replace(prefix, "")) }))
    .filter((entry) => entry.version)
    .sort((a, b) => compareVersions(b.version, a.version));

  if (tags.length === 0) return null;
  return { ...tags[0], versionText: formatVersion(tags[0].version) };
}

function resolveAgentTag(bump, latestTag) {
  const explicitVersion = normalizeExplicitVersion(bump, AGENT_TAG_PREFIX);
  if (explicitVersion) return `${AGENT_TAG_PREFIX}${explicitVersion}`;

  const latestVersion = latestTag.replace(AGENT_TAG_PREFIX, "");
  return `${AGENT_TAG_PREFIX}${resolveReleaseVersion(bump, latestVersion, AGENT_TAG_PREFIX)}`;
}

function resolveAppTag(value) {
  if (["patch", "minor", "major"].includes(value)) {
    fail("App distribution publishing requires an existing vX.Y.Z release tag, not a version bump.");
  }

  const explicitVersion = normalizeExplicitVersion(value, APP_TAG_PREFIX);
  if (!explicitVersion) {
    fail(`Invalid app release tag '${value}'. Use vX.Y.Z or X.Y.Z.`);
  }

  return `${APP_TAG_PREFIX}${explicitVersion}`;
}

function resolveReleaseVersion(bump, latestVersion, explicitTagPrefix) {
  const normalizedVersion = normalizeExplicitVersion(bump, explicitTagPrefix);
  if (normalizedVersion) return normalizedVersion;

  if (!["patch", "minor", "major"].includes(bump)) {
    fail(`Unknown version bump '${bump}'. Use patch, minor, major, or an explicit semver version.`);
  }

  const latest = parseVersion(latestVersion);
  if (!latest || latest.prerelease) {
    fail(`Cannot ${bump} bump from non-standard version '${latestVersion}'. Pass an explicit version instead.`);
  }

  if (bump === "major") return `${latest.major + 1}.0.0`;
  if (bump === "minor") return `${latest.major}.${latest.minor + 1}.0`;
  return `${latest.major}.${latest.minor}.${latest.patch + 1}`;
}

function normalizeExplicitVersion(value, explicitTagPrefix) {
  const trimmed = value.trim().replace(new RegExp(`^${escapeRegExp(explicitTagPrefix)}`), "").replace(/^v/, "");
  const version = parseVersion(trimmed);
  return version ? formatVersion(version) : null;
}

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(value);
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] ?? "",
  };
}

function formatVersion(version) {
  return `${version.major}.${version.minor}.${version.patch}${version.prerelease ? `-${version.prerelease}` : ""}`;
}

function compareVersions(a, b) {
  for (const key of ["major", "minor", "patch"]) {
    if (a[key] !== b[key]) return a[key] - b[key];
  }
  if (a.prerelease === b.prerelease) return 0;
  if (!a.prerelease) return 1;
  if (!b.prerelease) return -1;
  return a.prerelease.localeCompare(b.prerelease);
}

function tagExists(tag) {
  const result = spawnSync("git", ["rev-parse", "-q", "--verify", `refs/tags/${tag}`], {
    cwd: process.cwd(),
    encoding: "utf8",
    stdio: "pipe",
  });
  return result.status === 0;
}

function ensureGhReady(workflow) {
  run("gh", ["auth", "status", "--hostname", "github.com"], { stdio: "inherit" });
  run("gh", ["workflow", "view", workflow, "--repo", REPO], { stdio: "inherit" });
}

function fetchReleaseTags() {
  run("git", [
    "fetch",
    "--quiet",
    "origin",
    "+refs/tags/v*:refs/tags/v*",
    "+refs/tags/packages-v*:refs/tags/packages-v*",
    "+refs/tags/agents-v*:refs/tags/agents-v*",
  ]);
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: process.cwd(),
    env: process.env,
    encoding: "utf8",
    stdio: options.stdio ?? "pipe",
  });

  if (result.error) {
    fail(`${command} ${commandArgs.join(" ")} failed: ${result.error.message}`);
  }

  if (result.status !== 0) {
    const stderr = typeof result.stderr === "string" ? result.stderr.trim() : "";
    fail(`${command} ${commandArgs.join(" ")} failed${stderr ? `:\n${stderr}` : ""}`);
  }

  return result;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

function printHelp() {
  console.log(`Usage: node scripts/release.mjs [packages|agents|app] [patch|minor|major|version] [options]

Unified release trigger for DBX packages, agents, and app distribution.

Targets:
  packages              Trigger Node Packages Release via gh workflow run
  agents                Create and push an agents-v* tag
  app                   Trigger Publish Packages for an existing v* app release tag

Arguments:
  patch                 Bump the latest target tag by one patch version (default)
  minor                 Bump the latest target tag by one minor version
  major                 Bump the latest target tag by one major version
  0.4.14                Trigger an explicit version
  packages-v0.4.14      Explicit package tag style, for packages
  agents-v0.4.14        Explicit agent tag style, for agents
  v0.5.38               Existing app release tag, for app

Options:
  --dry-run             Print the release command without triggering it
  -y, --yes             Skip the confirmation prompt
  --skip-fetch          Do not run git fetch --tags before reading release tags
  -h, --help            Show this help
`);
}
