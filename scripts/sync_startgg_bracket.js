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
  const setSlotFields = await fetchTypeFields(endpoint, token, "SetSlot");
  return {
    entrantFields,
    participantFields,
    userFields,
    setSlotFields,
    hints: {
      entrantCustomFields: entrantFields?.has("customFields") ?? false,
      participantCustomFields: participantFields?.has("customFields") ?? false,
      userAuthorizations: userFields?.has("authorizations") ?? false,
      setSlotPrereqId: setSlotFields?.has("prereqId") ?? false,
      setSlotPrereqType: setSlotFields?.has("prereqType") ?? false,
      setSlotPrereqPlacement: setSlotFields?.has("prereqPlacement") ?? false,
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

async function fetchPhaseGroupSets(endpoint, token, phaseGroupId, schemaHints) {
  const prereqIdField = schemaHints?.setSlotPrereqId ? "prereqId" : "";
  const prereqTypeField = schemaHints?.setSlotPrereqType ? "prereqType" : "";
  const prereqPlacementField = schemaHints?.setSlotPrereqPlacement
    ? "prereqPlacement"
    : "";

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
              ${prereqIdField}
              ${prereqTypeField}
              ${prereqPlacementField}
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
  let pageInfo = null;
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
    pageInfo = eventData.entrants?.pageInfo ?? pageInfo;
    const nodes = eventData.entrants?.nodes ?? [];
    entrants.push(...nodes);
    totalPages = pageInfo?.totalPages ?? 1;
    page += 1;
  }

  return { event, entrants, pageInfo };
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
    name: "minimal",
    query: `
      query EventEntrantsMinimal($eventSlug: String!, $page: Int!, $perPage: Int!) {
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
              }
            }
          }
        }
      }
    `,
  },
];

async function fetchEventEntrants(endpoint, token, eventSlug, schemaHints) {
  const query = buildEntrantQueryFromSchema(schemaHints);
  try {
    const result = await fetchEventEntrantsWithQuery(endpoint, token, eventSlug, query);
    return { ...result, queryMode: "schemaAware" };
  } catch (err) {
    const msg = err?.message ? String(err.message) : String(err);
    if (!shouldFallbackEntrantQuery(msg)) {
      throw err;
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

function normalizeCode(value) {
  const trimmed = String(value ?? "").trim();
  if (!trimmed) return "";
  return trimmed.toUpperCase();
}

function normalizeName(value) {
  return String(value ?? "").trim().toLowerCase();
}

function toNumber(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function main() {
  loadEnvFile(path.join(REPO_ROOT, ".env"));

  const configArg = getArg("--file") ?? getArg("-f") ?? process.argv[2];
  const configPath = path.resolve(process.cwd(), configArg ?? DEFAULT_CONFIG);
  if (!fs.existsSync(configPath)) {
    throw new Error(`Config file not found: ${configPath}`);
  }

  const raw = fs.readFileSync(configPath, "utf8");
  const config = JSON.parse(raw);
  const link =
    getArg("--link") ??
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

  return (async () => {
    const schemaData = await fetchSchemaHints(endpoint, token);
    const schemaHints = schemaData.hints;
    const { phase, sets } = await fetchPhaseGroupSets(
      endpoint,
      token,
      parsed.phaseGroupId,
      schemaHints
    );
    const { event, entrants, pageInfo, queryMode } = await fetchEventEntrants(
      endpoint,
      token,
      parsed.eventSlugFull,
      schemaHints
    );

    const existingEntrants = Array.isArray(config.entrants) ? config.entrants : [];
    const existingCodesById = new Map();
    const existingCodesByName = new Map();
    const existingSeedsById = new Map();
    for (const entrant of existingEntrants) {
      const id = toNumber(entrant?.id);
      const code = normalizeCode(entrant?.slippiCode);
      const nameKey = normalizeName(entrant?.name);
      if (id !== null && code) {
        existingCodesById.set(id, code);
      }
      if (nameKey && code) {
        existingCodesByName.set(nameKey, code);
      }
      const seed = toNumber(entrant?.seed);
      if (id !== null && seed !== null) {
        existingSeedsById.set(id, seed);
      }
    }

    const nextEntrants = entrants.map((entrant, index) => {
      const id = toNumber(entrant?.id) ?? index + 1;
      const name = entrant?.name ?? "Unknown";
      const seedNum = toNumber(entrant?.seeds?.[0]?.seedNum);
      const seed = seedNum ?? existingSeedsById.get(id) ?? index + 1;
      const found = findSlippiCode(entrant);
      const existingCode =
        existingCodesById.get(id) ??
        existingCodesByName.get(normalizeName(name)) ??
        "";
      const slippiCode = normalizeCode(existingCode || found?.code || "");
      return { id, name, slippiCode, seed };
    });

    const usedCodes = new Set();
    for (const entrant of nextEntrants) {
      if (entrant.slippiCode && usedCodes.has(entrant.slippiCode)) {
        entrant.slippiCode = "";
        continue;
      }
      if (entrant.slippiCode) {
        usedCodes.add(entrant.slippiCode);
      }
    }

    const phaseId = toNumber(parsed.phaseId) ?? parsed.phaseId;
    const phaseGroupId = toNumber(parsed.phaseGroupId) ?? parsed.phaseGroupId;
    const bestOf =
      toNumber(config?.phases?.[0]?.bestOf) ??
      toNumber(config?.simulation?.bestOf) ??
      3;

    config.event = {
      id: String(event?.id ?? ""),
      name: event?.name ?? "Start.gg Event",
      slug: event?.slug ?? parsed.eventSlugFull ?? "",
    };
    config.phases = [
      {
        id: String(phase?.id ?? parsed.phaseId ?? "phase-1"),
        name: phase?.name ?? config?.phases?.[0]?.name ?? "Bracket",
        bestOf,
      },
    ];
    config.entrants = nextEntrants;

    config.referenceTournamentLink = link;
    config.referenceMetadata = {
      source: "start.gg",
      link,
      tournamentSlug: parsed.tournamentSlug ?? null,
      eventSlug: parsed.eventSlug ?? null,
      eventSlugFull: parsed.eventSlugFull ?? null,
      phaseId,
      phaseGroupId,
      phase: phase ? { id: phase.id, name: phase.name } : null,
      event: event ? { id: event.id, name: event.name, slug: event.slug } : null,
      totalEntrants: entrants.length,
      entrantQueryMode: queryMode ?? "unknown",
      slippiCodesFound: nextEntrants.filter((entrant) => entrant.slippiCode).length,
      schemaHints,
      slippiCodesFromCsv: config?.referenceMetadata?.slippiCodesFromCsv ?? 0,
      csvApplied: config?.referenceMetadata?.csvApplied ?? false,
      fetchedAt: new Date().toISOString(),
      totalSets: sets.length,
      slippiCodesFromReplays: config?.referenceMetadata?.slippiCodesFromReplays ?? 0,
    };

    config.referenceSets = sets.map((set) => ({
      id: set?.id ?? null,
      round: set?.round ?? null,
      fullRoundText: set?.fullRoundText ?? null,
      state: set?.state ?? null,
      startedAt: set?.startedAt ?? null,
      completedAt: set?.completedAt ?? null,
      winnerId: set?.winnerId ?? null,
      slots: (set?.slots ?? []).map((slot) => ({
        entrant: slot?.entrant ? { id: slot.entrant.id, name: slot.entrant.name } : null,
        standing: slot?.standing
          ? {
              stats: {
                score: slot?.standing?.stats?.score
                  ? {
                      value: slot.standing.stats.score.value ?? null,
                      label: slot.standing.stats.score.label ?? null,
                    }
                  : null,
              },
            }
          : null,
        prereqId: toNumber(slot?.prereqId),
        prereqType: slot?.prereqType ?? null,
        prereqPlacement: toNumber(slot?.prereqPlacement),
      })),
    }));

    const totalEntrants = entrants.length;
    const totalPages =
      pageInfo?.totalPages ?? (totalEntrants === 0 ? 0 : Math.ceil(totalEntrants / PER_PAGE));
    const total = pageInfo?.total ?? totalEntrants;

    config.referenceEvent = {
      id: event?.id ?? null,
      name: event?.name ?? "Start.gg Event",
      slug: event?.slug ?? parsed.eventSlugFull ?? "",
      entrants: {
        pageInfo: {
          totalPages,
          total,
        },
        nodes: entrants,
      },
    };

    config.referenceEntrants = entrants;

    const outputPath = hasFlag("--out")
      ? path.resolve(process.cwd(), getArg("--out"))
      : configPath;
    fs.writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`);
    const relPath = path.relative(process.cwd(), outputPath);
    console.log(`Updated ${relPath} (entrants=${nextEntrants.length}, sets=${sets.length}).`);
  })();
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
