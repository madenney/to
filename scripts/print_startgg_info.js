#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DEFAULT_ENDPOINT = "https://api.start.gg/gql/alpha";
const PER_PAGE = 100;
const SCRIPT_PATH = fileURLToPath(import.meta.url);
const SCRIPT_DIR = path.dirname(SCRIPT_PATH);
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const DEFAULT_CONFIG = path.join(REPO_ROOT, "test_brackets", "test_bracket_2.json");

function getArg(flag) {
  const idx = process.argv.indexOf(flag);
  if (idx === -1 || idx + 1 >= process.argv.length) return null;
  return process.argv[idx + 1];
}

function hasFlag(flag) {
  return process.argv.includes(flag);
}

function loadEnvFile(envPath) {
  if (!fs.existsSync(envPath)) return;
  const lines = fs.readFileSync(envPath, "utf8").split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    const eq = trimmed.indexOf("=");
    if (eq === -1) {
      continue;
    }
    const key = trimmed.slice(0, eq).trim();
    let value = trimmed.slice(eq + 1).trim();
    if (
      (value.startsWith("\"") && value.endsWith("\"")) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    if (!process.env[key]) {
      process.env[key] = value;
    }
  }
}

function parseReferenceLink(link) {
  const url = new URL(link);
  const parts = url.pathname.split("/").filter(Boolean);
  const tournamentIdx = parts.indexOf("tournament");
  const eventIdx = parts.indexOf("event");
  const bracketsIdx = parts.indexOf("brackets");
  const tournamentSlug = tournamentIdx >= 0 ? parts[tournamentIdx + 1] : null;
  const eventSlug = eventIdx >= 0 ? parts[eventIdx + 1] : null;
  const phaseId = bracketsIdx >= 0 ? parts[bracketsIdx + 1] : null;
  const phaseGroupId = bracketsIdx >= 0 ? parts[bracketsIdx + 2] : null;
  const eventSlugFull =
    tournamentSlug && eventSlug
      ? `tournament/${tournamentSlug}/event/${eventSlug}`
      : null;
  return { tournamentSlug, eventSlug, eventSlugFull, phaseId, phaseGroupId };
}

async function fetchGraphql(endpoint, token, query, variables) {
  const res = await fetch(endpoint, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Start.gg request failed (${res.status}): ${text}`);
  }
  const json = await res.json();
  if (Array.isArray(json.errors) && json.errors.length > 0) {
    throw new Error(json.errors[0]?.message ?? "Start.gg query failed.");
  }
  return json.data;
}

async function fetchTypeFields(endpoint, token, typeName) {
  const query = `
    query TypeFields($name: String!) {
      __type(name: $name) {
        fields {
          name
        }
      }
    }
  `;
  try {
    const data = await fetchGraphql(endpoint, token, query, { name: typeName });
    const fields = data?.__type?.fields ?? [];
    return new Set(fields.map((field) => field?.name).filter(Boolean));
  } catch {
    return null;
  }
}

async function fetchSchemaHints(endpoint, token) {
  const entrantFields = await fetchTypeFields(endpoint, token, "Entrant");
  const participantFields = await fetchTypeFields(endpoint, token, "Participant");
  const userFields = await fetchTypeFields(endpoint, token, "User");
  return {
    entrantFields,
    participantFields,
    userFields,
    hints: {
      entrantCustomFields: entrantFields?.has("customFields") ?? false,
      participantCustomFields: participantFields?.has("customFields") ?? false,
      userAuthorizations: userFields?.has("authorizations") ?? false,
      hasEntrantFields: Boolean(entrantFields),
    },
  };
}

function buildEntrantQueryFromSchema(schema) {
  const entrantCustomFields = schema.entrantCustomFields
    ? `
              customFields {
                name
                value
              }
    `
    : "";
  const participantCustomFields = schema.participantCustomFields
    ? `
                customFields {
                  name
                  value
                }
    `
    : "";
  const userAuthorizations = schema.userAuthorizations
    ? `
                  authorizations {
                    type
                    externalUsername
                  }
    `
    : "";

  return `
    query EventEntrantsSchemaAware($eventSlug: String!, $page: Int!, $perPage: Int!) {
      event(slug: $eventSlug) {
        id
        name
        slug
        entrants(query: { page: $page, perPage: $perPage }) {
          pageInfo {
            totalPages
            total
          }
          nodes {
            id
            name
            seeds {
              seedNum
            }
            ${entrantCustomFields}
            participants {
              id
              gamerTag
              ${participantCustomFields}
              player {
                gamerTag
              }
              user {
                id
                slug
                ${userAuthorizations}
              }
            }
          }
        }
      }
    }
  `;
}

async function fetchPhaseGroupSets(endpoint, token, phaseGroupId) {
  const query = `
    query PhaseGroupSets($phaseGroupId: ID!, $page: Int!, $perPage: Int!) {
      phaseGroup(id: $phaseGroupId) {
        id
        phase {
          id
          name
        }
        sets(page: $page, perPage: $perPage, sortType: STANDARD) {
          pageInfo {
            total
            totalPages
          }
          nodes {
            id
            round
            fullRoundText
            state
            startedAt
            completedAt
            winnerId
            slots {
              entrant {
                id
                name
              }
              standing {
                stats {
                  score {
                    value
                    label
                  }
                }
              }
            }
          }
        }
      }
    }
  `;

  let page = 1;
  let totalPages = 1;
  let phase = null;
  const sets = [];

  while (page <= totalPages) {
    const data = await fetchGraphql(endpoint, token, query, {
      phaseGroupId,
      page,
      perPage: PER_PAGE,
    });
    const group = data?.phaseGroup;
    if (!group) {
      throw new Error(`Phase group ${phaseGroupId} not found.`);
    }
    phase = group.phase ?? phase;
    const pageInfo = group.sets?.pageInfo;
    const nodes = group.sets?.nodes ?? [];
    sets.push(...nodes);
    totalPages = pageInfo?.totalPages ?? 1;
    page += 1;
  }

  return { phase, sets };
}

function shouldFallbackEntrantQuery(message) {
  const lower = message.toLowerCase();
  return (
    lower.includes("cannot query field") ||
    lower.includes("unknown argument") ||
    lower.includes("not authorized") ||
    lower.includes("permission") ||
    lower.includes("requires") ||
    lower.includes("forbidden")
  );
}

async function fetchEventEntrantsWithQuery(endpoint, token, eventSlug, query) {
  let page = 1;
  let totalPages = 1;
  let event = null;
  const entrants = [];

  while (page <= totalPages) {
    const data = await fetchGraphql(endpoint, token, query, {
      eventSlug,
      page,
      perPage: PER_PAGE,
    });
    const eventData = data?.event;
    if (!eventData) {
      throw new Error(`Event not found for slug ${eventSlug}.`);
    }
    event = eventData;
    const pageInfo = eventData.entrants?.pageInfo;
    const nodes = eventData.entrants?.nodes ?? [];
    entrants.push(...nodes);
    totalPages = pageInfo?.totalPages ?? 1;
    page += 1;
  }

  return { event, entrants };
}

const ENTRANT_QUERY_PROFILES = [
  {
    name: "full",
    query: `
      query EventEntrantsFull($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              customFields {
                name
                value
              }
              participants {
                id
                gamerTag
                customFields {
                  name
                  value
                }
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                  authorizations {
                    type
                    externalUsername
                  }
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "noParticipantCustomFields",
    query: `
      query EventEntrantsNoParticipantCustom($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              customFields {
                name
                value
              }
              participants {
                id
                gamerTag
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                  authorizations {
                    type
                    externalUsername
                  }
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "authorizationsOnly",
    query: `
      query EventEntrantsAuthOnly($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              participants {
                id
                gamerTag
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                  authorizations {
                    type
                    externalUsername
                  }
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "noAuthorizations",
    query: `
      query EventEntrantsNoAuth($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              customFields {
                name
                value
              }
              participants {
                id
                gamerTag
                customFields {
                  name
                  value
                }
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "entrantCustomFieldsOnly",
    query: `
      query EventEntrantsEntrantCustom($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              customFields {
                name
                value
              }
              participants {
                id
                gamerTag
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "participantCustomFieldsOnly",
    query: `
      query EventEntrantsParticipantCustom($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              participants {
                id
                gamerTag
                customFields {
                  name
                  value
                }
                player {
                  gamerTag
                }
                user {
                  id
                  slug
                }
              }
            }
          }
        }
      }
    `,
  },
  {
    name: "basic",
    query: `
      query EventEntrantsBasic($eventSlug: String!, $page: Int!, $perPage: Int!) {
        event(slug: $eventSlug) {
          id
          name
          slug
          entrants(query: { page: $page, perPage: $perPage }) {
            pageInfo {
              totalPages
              total
            }
            nodes {
              id
              name
              seeds {
                seedNum
              }
              participants {
                id
                gamerTag
              }
            }
          }
        }
      }
    `,
  },
];

async function fetchEventEntrants(endpoint, token, eventSlug, schemaHints) {
  if (schemaHints.hasEntrantFields) {
    if (!schemaHints.entrantCustomFields && !schemaHints.participantCustomFields && !schemaHints.userAuthorizations) {
      throw new Error(
        "Start.gg permissions issue: this token's schema does not expose customFields or user.authorizations."
      );
    }
    const schemaQuery = buildEntrantQueryFromSchema(schemaHints);
    try {
      const result = await fetchEventEntrantsWithQuery(endpoint, token, eventSlug, schemaQuery);
      return { ...result, queryMode: "schemaAware" };
    } catch (err) {
      const msg = err?.message ? String(err.message) : String(err);
      if (!shouldFallbackEntrantQuery(msg)) {
        throw err;
      }
      console.warn(`Schema-aware entrant query failed: ${msg}`);
    }
  }

  let lastError = null;
  for (const profile of ENTRANT_QUERY_PROFILES) {
    try {
      const result = await fetchEventEntrantsWithQuery(
        endpoint,
        token,
        eventSlug,
        profile.query
      );
      return { ...result, queryMode: profile.name };
    } catch (err) {
      const msg = err?.message ? String(err.message) : String(err);
      if (!shouldFallbackEntrantQuery(msg)) {
        throw err;
      }
      console.warn(`Entrant query '${profile.name}' failed: ${msg}`);
      lastError = err;
    }
  }
  throw lastError ?? new Error("Failed to load entrants from Start.gg.");
}

function normalizeFieldValue(value) {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

function extractFromCustomFields(fields) {
  for (const field of fields ?? []) {
    const name = (field?.name ?? "").toLowerCase();
    if (name.includes("slippi") || name.includes("connect")) {
      const value = normalizeFieldValue(field?.value);
      if (value) {
        return value;
      }
    }
  }
  return null;
}

function findSlippiCode(entrant) {
  if (entrant?.slippiCode) {
    return { code: entrant.slippiCode, source: "entrant.slippiCode" };
  }
  const fromEntrantFields = extractFromCustomFields(entrant?.customFields);
  if (fromEntrantFields) {
    return { code: fromEntrantFields, source: "entrant.customFields" };
  }
  for (const participant of entrant?.participants ?? []) {
    const fromParticipantFields = extractFromCustomFields(participant?.customFields);
    if (fromParticipantFields) {
      return { code: fromParticipantFields, source: "participant.customFields" };
    }
    for (const auth of participant?.user?.authorizations ?? []) {
      const authType = (auth?.type ?? "").toLowerCase();
      if (authType.includes("slippi") || authType.includes("connect")) {
        if (auth?.externalUsername) {
          return { code: auth.externalUsername, source: "user.authorizations" };
        }
      }
    }
    const tags = [participant?.gamerTag, participant?.player?.gamerTag];
    for (const tag of tags) {
      if (tag && tag.includes("#")) {
        return { code: tag, source: "participant.tag" };
      }
    }
  }
  return null;
}

function formatBool(value) {
  return value ? "yes" : "no";
}

function formatList(values, maxItems = 10) {
  const list = Array.from(values ?? []).filter(Boolean);
  if (list.length === 0) return "none";
  const shown = list.slice(0, maxItems);
  const suffix = list.length > maxItems ? `, +${list.length - maxItems} more` : "";
  return `${shown.join(", ")}${suffix}`;
}

function formatCountMap(map, maxItems = 10) {
  const entries = Array.from(map.entries());
  if (entries.length === 0) return "none";
  entries.sort((a, b) => b[1] - a[1]);
  const shown = entries.slice(0, maxItems);
  const suffix = entries.length > maxItems ? `, +${entries.length - maxItems} more` : "";
  return `${shown.map(([key, count]) => `${key}=${count}`).join(", ")}${suffix}`;
}

function collectFieldNames(entrants) {
  const entrantFields = new Set();
  const participantFields = new Set();
  for (const entrant of entrants) {
    for (const field of entrant?.customFields ?? []) {
      if (field?.name) entrantFields.add(field.name);
    }
    for (const participant of entrant?.participants ?? []) {
      for (const field of participant?.customFields ?? []) {
        if (field?.name) participantFields.add(field.name);
      }
    }
  }
  return { entrantFields, participantFields };
}

function pickSamples(items, limit) {
  if (items.length <= limit) return items;
  return items.slice(0, limit);
}

function formatTimestamp(unixSeconds) {
  if (!Number.isFinite(unixSeconds)) return null;
  return new Date(unixSeconds * 1000).toISOString();
}

async function main() {
  loadEnvFile(path.join(REPO_ROOT, ".env"));

  const configArg = getArg("--file") ?? getArg("-f") ?? process.argv[2];
  const configPath = path.resolve(process.cwd(), configArg ?? DEFAULT_CONFIG);
  if (!fs.existsSync(configPath)) {
    throw new Error(`Config file not found: ${configPath}`);
  }

  const raw = fs.readFileSync(configPath, "utf8");
  const config = JSON.parse(raw);
  const link =
    config.referenceTournamentLink ??
    config.referenceTournament?.link ??
    null;
  if (!link) {
    throw new Error("Missing referenceTournamentLink in the config file.");
  }

  const token =
    process.env.STARTGG_TOKEN ||
    process.env.STARTGG_API_TOKEN ||
    process.env.STARTGG_AUTH_TOKEN;
  if (!token) {
    throw new Error("Missing Start.gg token. Set STARTGG_TOKEN (or STARTGG_API_TOKEN).");
  }

  const endpoint = process.env.STARTGG_ENDPOINT || DEFAULT_ENDPOINT;
  const parsed = parseReferenceLink(link);
  if (!parsed.phaseGroupId) {
    throw new Error("referenceTournamentLink must include /brackets/<phaseId>/<phaseGroupId>.");
  }
  if (!parsed.eventSlugFull) {
    throw new Error("referenceTournamentLink must include /tournament/<slug>/event/<event-slug>.");
  }

  const showFields = hasFlag("--fields") || hasFlag("--verbose");
  const showSamples = hasFlag("--samples") || hasFlag("--verbose");

  const schemaData = await fetchSchemaHints(endpoint, token);
  const schemaHints = schemaData.hints;
  const { phase, sets } = await fetchPhaseGroupSets(endpoint, token, parsed.phaseGroupId);
  const { event, entrants, queryMode } = await fetchEventEntrants(
    endpoint,
    token,
    parsed.eventSlugFull,
    schemaHints
  );

  const entrantCount = entrants.length;
  const seeded = entrants.filter((entrant) =>
    Number.isFinite(entrant?.seeds?.[0]?.seedNum)
  ).length;

  const slippiSources = new Map();
  const slippiCodes = new Map();
  const missingCodes = [];

  for (const entrant of entrants) {
    const found = findSlippiCode(entrant);
    if (found?.code) {
      slippiCodes.set(entrant?.id ?? entrant?.name ?? found.code, {
        name: entrant?.name ?? "Unknown",
        code: String(found.code).toUpperCase(),
        source: found.source,
      });
      slippiSources.set(
        found.source,
        (slippiSources.get(found.source) ?? 0) + 1
      );
    } else {
      missingCodes.push(entrant?.name ?? "Unknown");
    }
  }

  const uniqueCodes = new Set(Array.from(slippiCodes.values()).map((entry) => entry.code));
  const { entrantFields, participantFields } = collectFieldNames(entrants);

  const authTypes = new Map();
  for (const entrant of entrants) {
    for (const participant of entrant?.participants ?? []) {
      for (const auth of participant?.user?.authorizations ?? []) {
        const type = auth?.type ?? "Unknown";
        authTypes.set(type, (authTypes.get(type) ?? 0) + 1);
      }
    }
  }

  const setStates = new Map();
  const roundLabels = new Map();
  let earliestStart = null;
  let latestEnd = null;

  for (const set of sets) {
    const stateKey = String(set?.state ?? "unknown");
    setStates.set(stateKey, (setStates.get(stateKey) ?? 0) + 1);
    const roundKey = String(set?.fullRoundText ?? set?.round ?? "unknown");
    roundLabels.set(roundKey, (roundLabels.get(roundKey) ?? 0) + 1);
    if (Number.isFinite(set?.startedAt)) {
      earliestStart = earliestStart === null ? set.startedAt : Math.min(earliestStart, set.startedAt);
    }
    if (Number.isFinite(set?.completedAt)) {
      latestEnd = latestEnd === null ? set.completedAt : Math.max(latestEnd, set.completedAt);
    }
  }

  const relPath = path.relative(process.cwd(), configPath);
  console.log(`Start.gg info for ${relPath}`);
  console.log(`Link: ${link}`);
  console.log(
    `Parsed: tournament=${parsed.tournamentSlug ?? "?"}, event=${parsed.eventSlug ?? "?"}, phaseId=${parsed.phaseId ?? "?"}, phaseGroupId=${parsed.phaseGroupId ?? "?"}`
  );
  console.log(`Endpoint: ${endpoint}`);
  console.log("");
  console.log("Schema:");
  console.log(
    `  entrantCustomFields=${formatBool(schemaHints.entrantCustomFields)}, participantCustomFields=${formatBool(schemaHints.participantCustomFields)}, userAuthorizations=${formatBool(schemaHints.userAuthorizations)}`
  );
  console.log(`  entrantFieldsVisible=${formatBool(schemaHints.hasEntrantFields)}`);
  if (showFields) {
    console.log(`  Entrant fields: ${formatList(schemaData.entrantFields ?? [])}`);
    console.log(`  Participant fields: ${formatList(schemaData.participantFields ?? [])}`);
    console.log(`  User fields: ${formatList(schemaData.userFields ?? [])}`);
  }
  console.log("");
  console.log("Event:");
  console.log(`  ${event?.name ?? "Unknown"} (id=${event?.id ?? "?"}, slug=${event?.slug ?? "?"})`);
  console.log(`  Phase: ${phase?.name ?? "Unknown"} (id=${phase?.id ?? "?"})`);
  console.log(`  Entrant query mode: ${queryMode ?? "unknown"}`);
  console.log("");
  console.log(
    `Entrants: total=${entrantCount}, seeded=${seeded}, slippiCodes=${slippiCodes.size}/${entrantCount}, uniqueCodes=${uniqueCodes.size}`
  );
  console.log(`  Slippi sources: ${formatCountMap(slippiSources)}`);
  console.log(`  Entrant custom fields: ${formatList(entrantFields)}`);
  console.log(`  Participant custom fields: ${formatList(participantFields)}`);
  console.log(`  Auth types: ${formatCountMap(authTypes)}`);
  if (showSamples) {
    const codeSamples = pickSamples(Array.from(slippiCodes.values()), 8);
    const missingSamples = pickSamples(missingCodes, 8);
    if (codeSamples.length > 0) {
      console.log("  Sample codes:");
      for (const entry of codeSamples) {
        console.log(`    - ${entry.name}: ${entry.code} (${entry.source})`);
      }
    }
    if (missingSamples.length > 0) {
      console.log("  Sample missing:");
      for (const name of missingSamples) {
        console.log(`    - ${name}`);
      }
    }
  }
  console.log("");
  console.log(`Sets: total=${sets.length}, states=${formatCountMap(setStates)}`);
  console.log(`  Rounds: ${formatCountMap(roundLabels, 12)}`);
  const startIso = formatTimestamp(earliestStart);
  const endIso = formatTimestamp(latestEnd);
  if (startIso || endIso) {
    console.log(`  Time range: ${startIso ?? "?"} to ${endIso ?? "?"}`);
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
