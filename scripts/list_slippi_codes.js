#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg.startsWith("--")) {
      const key = arg.slice(2);
      const next = argv[i + 1];
      if (!next || next.startsWith("-")) {
        args[key] = true;
      } else {
        args[key] = next;
        i += 1;
      }
      continue;
    }
    if (arg.startsWith("-")) {
      const key = arg.slice(1);
      const next = argv[i + 1];
      if (!next || next.startsWith("-")) {
        args[key] = true;
      } else {
        args[key] = next;
        i += 1;
      }
      continue;
    }
    args._.push(arg);
  }
  return args;
}

function printUsage() {
  console.log(
    [
      "Usage: list_slippi_codes [replays-dir | test-bracket.json] [options]",
      "",
      "Options:",
      "  --replays <dir>   Base replays folder (default: test_files/replays)",
      "  --event <name>    Event subfolder name (defaults to config event slug/name)",
      "  --slippi-node-path <dir> Node path that contains @slippi/slippi-js",
      "  --counts          Include replay counts per tag",
      "  --help            Show this help",
    ].join("\n")
  );
}

function slugify(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function normalizeCode(value) {
  const raw = String(value ?? "").trim();
  if (!raw) return null;
  return raw.toUpperCase();
}

function normalizePlayers(playersObj) {
  if (!playersObj) return [];
  if (Array.isArray(playersObj)) return playersObj;
  return Object.entries(playersObj)
    .map(([idx, data]) => ({ idx: Number(idx), data }))
    .sort((a, b) => a.idx - b.idx)
    .map((entry) => entry.data);
}

function collectSlpFiles(dirPath) {
  const entries = fs.readdirSync(dirPath, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(dirPath, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectSlpFiles(fullPath));
      continue;
    }
    if (!entry.isFile()) continue;
    const ext = path.extname(entry.name).toLowerCase();
    if (ext === ".slp" || ext === ".slippi") {
      files.push(fullPath);
    }
  }
  return files;
}

function codeTag(code) {
  const raw = String(code ?? "");
  const idx = raw.indexOf("#");
  if (idx === -1) return raw;
  return raw.slice(0, idx);
}

function resolveSlippiModule(explicitPath) {
  const require = createRequire(import.meta.url);
  const candidates = [];
  const repoRoot = path.resolve(process.cwd());

  if (explicitPath) {
    const resolved = path.resolve(explicitPath);
    if (resolved.endsWith(`${path.sep}@slippi${path.sep}slippi-js`)) {
      candidates.push(resolved);
    } else {
      candidates.push(path.join(resolved, "@slippi", "slippi-js"));
      candidates.push(resolved);
    }
  }

  const envPath = process.env.SLIPPI_NODE_PATH ?? process.env.NODE_PATH ?? "";
  envPath
    .split(path.delimiter)
    .map((entry) => entry.trim())
    .filter(Boolean)
    .forEach((entry) => {
      candidates.push(path.join(entry, "@slippi", "slippi-js"));
      candidates.push(entry);
    });

  candidates.push(path.join(repoRoot, "node_modules", "@slippi", "slippi-js"));
  candidates.push(
    path.resolve(repoRoot, "..", "replay_archiver", "node_modules", "@slippi", "slippi-js")
  );

  for (const candidate of candidates) {
    if (!candidate || !fs.existsSync(candidate)) continue;
    try {
      const mod = require(candidate);
      const SlippiGame = mod.SlippiGame || mod?.default?.SlippiGame;
      if (SlippiGame) {
        return { SlippiGame, source: candidate };
      }
    } catch {
      // try next
    }
  }

  throw new Error(
    "Unable to load @slippi/slippi-js. Provide --slippi-node-path or set SLIPPI_NODE_PATH."
  );
}

function resolveTag(names) {
  if (!names) return null;
  return (
    names.netplay ||
    names.displayName ||
    names.nickname ||
    names.tag ||
    names.name ||
    null
  );
}

function resolveReplaysDir(args) {
  const arg0 = args._[0];
  const configPath =
    args.config ??
    args.c ??
    (arg0 && arg0.endsWith(".json") ? arg0 : null);

  if (configPath) {
    const fullConfigPath = path.resolve(process.cwd(), configPath);
    if (!fs.existsSync(fullConfigPath)) {
      throw new Error(`Config not found: ${fullConfigPath}`);
    }
    const config = JSON.parse(fs.readFileSync(fullConfigPath, "utf8"));
    const eventName =
      args.event ??
      config?.event?.slug ??
      config?.event?.name ??
      null;
    const replaysBase = path.resolve(process.cwd(), args.replays ?? "test_files/replays");
    if (eventName) {
      const candidate = path.join(replaysBase, slugify(eventName));
      if (fs.existsSync(candidate) && fs.statSync(candidate).isDirectory()) {
        return candidate;
      }
      if (args.event) {
        throw new Error(`Event replays folder not found: ${candidate}`);
      }
    }
    return replaysBase;
  }

  if (args.event) {
    const replaysBase = path.resolve(process.cwd(), args.replays ?? arg0 ?? "test_files/replays");
    return path.join(replaysBase, slugify(args.event));
  }

  if (arg0) {
    return path.resolve(process.cwd(), arg0);
  }

  return path.resolve(process.cwd(), args.replays ?? "test_files/replays");
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help || args.h) {
    printUsage();
    return;
  }

  const replaysDir = resolveReplaysDir(args);
  if (!fs.existsSync(replaysDir) || !fs.statSync(replaysDir).isDirectory()) {
    throw new Error(`Replays folder not found: ${replaysDir}`);
  }

  const { SlippiGame } = resolveSlippiModule(
    args["slippi-node-path"] ?? args.slippiNodePath
  );
  const files = collectSlpFiles(replaysDir);
  if (files.length === 0) {
    console.log("No .slp files found.");
    return;
  }

  const counts = new Map();
  for (const filePath of files) {
    let game;
    try {
      game = new SlippiGame(filePath);
    } catch {
      continue;
    }
    const meta = game.getMetadata?.() ?? {};
    const players = normalizePlayers(meta.players);
    const seen = new Set();
    for (const player of players) {
      const names = player?.names ?? {};
      const code = normalizeCode(names.code);
      if (!code) continue;
      const tag = resolveTag(names);
      if (!tag) continue;
      const key = `${code}::${tag}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const perCode = counts.get(code) ?? new Map();
      perCode.set(tag, (perCode.get(tag) ?? 0) + 1);
      counts.set(code, perCode);
    }
  }

  const sorted = Array.from(counts.entries()).sort((a, b) => a[0].localeCompare(b[0]));
  const showCounts = Boolean(args.counts);
  const maxCodeLen = sorted.reduce((max, [code]) => Math.max(max, code.length), 0);
  const gap = "  ";
  for (const [code, tagMap] of sorted) {
    const tags = Array.from(tagMap.entries()).sort((a, b) => {
      if (a[1] !== b[1]) return b[1] - a[1];
      return a[0].localeCompare(b[0]);
    });
    if (tags.length === 0) continue;
    const codeLabel = code.padEnd(maxCodeLen, " ");
    if (showCounts) {
      const rendered = tags.map(([tag, count]) => `${tag}(${count})`).join(", ");
      console.log(`${codeLabel}${gap}${rendered}`);
    } else {
      const rendered = tags.map(([tag]) => tag).join(", ");
      console.log(`${codeLabel}${gap}${rendered}`);
    }
  }
}

Promise.resolve(main()).catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
