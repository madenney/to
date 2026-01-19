#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (value.startsWith("--")) {
      const key = value.slice(2);
      const next = argv[i + 1];
      if (!next || next.startsWith("-")) {
        args[key] = true;
      } else {
        args[key] = next;
        i += 1;
      }
      continue;
    }
    args._.push(value);
  }
  return args;
}

function printUsage() {
  console.log(
    [
      "Usage: scan_bracket_replays <bracket.json> <replays-dir> [options]",
      "",
      "Options:",
      "  --out <path>         Write output to this file (default: overwrite bracket.json)",
      "  --max-per-set <num>  Max replay paths to attach per set (default: 3)",
      "  --help               Show this help",
    ].join("\n")
  );
}

function collectSlpFiles(rootDir) {
  const files = [];
  const stack = [rootDir];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!current) continue;
    const entries = fs.readdirSync(current, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(fullPath);
        continue;
      }
      if (!entry.isFile()) {
        continue;
      }
      const ext = path.extname(entry.name).toLowerCase();
      if (ext === ".slp" || ext === ".slippi") {
        files.push(fullPath);
      }
    }
  }
  files.sort();
  return files;
}

function isAlphaNum(byte) {
  return (
    (byte >= 48 && byte <= 57) ||
    (byte >= 65 && byte <= 90) ||
    (byte >= 97 && byte <= 122)
  );
}

function isDigit(byte) {
  return byte >= 48 && byte <= 57;
}

function extractConnectCodes(buffer) {
  const codes = [];
  let i = 0;
  while (i < buffer.length) {
    if (buffer[i] !== 35) {
      i += 1;
      continue;
    }
    let start = i;
    while (start > 0 && isAlphaNum(buffer[start - 1])) {
      start -= 1;
    }
    let end = i + 1;
    while (end < buffer.length && isDigit(buffer[end])) {
      end += 1;
    }
    const leftLen = i - start;
    const rightLen = end - (i + 1);
    if (leftLen >= 2 && leftLen <= 12 && rightLen >= 3 && rightLen <= 4) {
      const left = buffer.toString("ascii", start, i);
      const right = buffer.toString("ascii", i + 1, end);
      codes.push(`${left}#${right}`);
    }
    i = end;
  }
  return codes;
}

function normalizeCode(code) {
  const trimmed = String(code ?? "").trim();
  if (!trimmed) return null;
  return trimmed.toUpperCase();
}

function replayPairKey(left, right) {
  return left <= right ? `${left}|${right}` : `${right}|${left}`;
}

function loadJson(jsonPath) {
  const raw = fs.readFileSync(jsonPath, "utf8");
  return JSON.parse(raw);
}

function writeJson(jsonPath, data) {
  fs.writeFileSync(jsonPath, `${JSON.stringify(data, null, 2)}\n`);
}

function buildReplayMap(files) {
  const map = new Map();
  for (const file of files) {
    const buffer = fs.readFileSync(file);
    const rawCodes = extractConnectCodes(buffer);
    const unique = new Set();
    for (const code of rawCodes) {
      const normalized = normalizeCode(code);
      if (normalized) {
        unique.add(normalized);
      }
    }
    const codes = Array.from(unique);
    if (codes.length < 2) {
      continue;
    }
    for (let i = 0; i < codes.length; i += 1) {
      for (let j = i + 1; j < codes.length; j += 1) {
        const key = replayPairKey(codes[i], codes[j]);
        const list = map.get(key) ?? [];
        list.push(file);
        map.set(key, list);
      }
    }
  }
  for (const [key, list] of map.entries()) {
    const unique = Array.from(new Set(list));
    unique.sort();
    map.set(key, unique);
  }
  return map;
}

function resolveSlotCode(slot, entrantsById) {
  const entrantId = slot?.entrant?.id ?? slot?.entrantId ?? null;
  const entrant = entrantId ? entrantsById.get(Number(entrantId)) : null;
  return normalizeCode(entrant?.slippiCode);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help || args._.length < 2) {
    printUsage();
    process.exit(args.help ? 0 : 1);
  }

  const bracketPath = path.resolve(process.cwd(), args._[0]);
  const replaysDirRaw = args._[1];
  const replaysDir = path.resolve(process.cwd(), replaysDirRaw);
  const repoRoot = process.cwd();
  const outPath = path.resolve(
    repoRoot,
    args.out ? args.out : args._[0]
  );
  const maxPerSet = Number(args["max-per-set"] ?? 3);

  if (!fs.existsSync(bracketPath)) {
    throw new Error(`Bracket config not found: ${bracketPath}`);
  }
  if (!fs.existsSync(replaysDir) || !fs.statSync(replaysDir).isDirectory()) {
    throw new Error(`Replay folder not found: ${replaysDir}`);
  }
  if (!Number.isFinite(maxPerSet) || maxPerSet < 1) {
    throw new Error("--max-per-set must be a positive number.");
  }

  const bracket = loadJson(bracketPath);
  const entrants = Array.isArray(bracket.entrants) ? bracket.entrants : [];
  const entrantsById = new Map(
    entrants.map((entrant) => [Number(entrant.id), entrant])
  );
  const referenceSets = Array.isArray(bracket.referenceSets)
    ? bracket.referenceSets
    : [];
  if (referenceSets.length === 0) {
    throw new Error("Bracket config has no referenceSets to match against.");
  }

  const files = collectSlpFiles(replaysDir);
  const replayMap = buildReplayMap(files);
  const replaySets = [];
  let matchedSets = 0;
  let matchedReplays = 0;

  for (const set of referenceSets) {
    const slots = Array.isArray(set.slots) ? set.slots : [];
    if (slots.length < 2) {
      continue;
    }
    const codes = slots.map((slot) => resolveSlotCode(slot, entrantsById)).filter(Boolean);
    if (codes.length < 2) {
      continue;
    }
    const left = codes[0];
    const right = codes[1];
    const key = replayPairKey(left, right);
    const matched = replayMap.get(key) ?? [];
    if (matched.length === 0) {
      continue;
    }
    const selected = matched.slice(0, maxPerSet);
    matchedSets += 1;
    matchedReplays += selected.length;
    const replayEntries = selected.map((replayPath) => {
      const relative = path.relative(repoRoot, replayPath);
      const storedPath = relative && !relative.startsWith("..") ? relative : replayPath;
      return {
        path: storedPath,
        slots: [{ slippiCode: left }, { slippiCode: right }],
      };
    });
    replaySets.push({
      id: set.id,
      round: set.round ?? null,
      fullRoundText: set.fullRoundText ?? null,
      replays: replayEntries,
    });
  }

  bracket.referenceReplayMap = {
    source: "slp",
    generatedAt: new Date().toISOString(),
    replaysDir: replaysDirRaw,
    matchedSets,
    matchedReplays,
    totalSets: referenceSets.length,
    totalReplays: files.length,
    maxPerSet,
    sets: replaySets,
  };

  writeJson(outPath, bracket);
  console.log(
    `Updated ${outPath} (${matchedSets}/${referenceSets.length} sets, ${matchedReplays} replays).`
  );
}

try {
  main();
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  console.error(message);
  process.exit(1);
}
