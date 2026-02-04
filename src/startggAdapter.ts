import type { StartggSimState } from "./types/overlay";

type StartggRawResponse = {
  data?: {
    event?: StartggRawEvent | null;
  };
  extensions?: {
    nowMs?: number;
    startedAtMs?: number;
    eventLink?: string | null;
  };
};

type StartggRawEvent = {
  id?: string | number;
  name?: string | null;
  slug?: string | null;
  phases?: StartggRawPhase[] | { nodes?: StartggRawPhase[] };
  entrants?: { nodes?: StartggRawEntrant[] };
  sets?: { nodes?: StartggRawSet[] };
};

type StartggRawPhase = {
  id?: string | number;
  name?: string | null;
  bestOf?: number | null;
};

type StartggRawEntrant = {
  id?: string | number;
  name?: string | null;
  seeds?: Array<{ seedNum?: number | null }>;
  seed?: number | null;
  slippiCode?: string | null;
  customFields?: Array<{ name?: string | null; value?: string | null }>;
  participants?: StartggRawParticipant[];
};

type StartggRawParticipant = {
  id?: string | number;
  gamerTag?: string | null;
  player?: { gamerTag?: string | null; slippiCode?: string | null };
  user?: {
    id?: string | number;
    slug?: string | null;
    authorizations?: Array<{ type?: string | null; externalUsername?: string | null }>;
  };
};

type StartggRawSet = {
  id?: string | number;
  round?: number | null;
  fullRoundText?: string | null;
  state?: number | string | null;
  startedAt?: number | null;
  completedAt?: number | null;
  updatedAt?: number | null;
  winnerId?: number | string | null;
  phaseGroup?: { phase?: { id?: string | number; name?: string | null } };
  slots?: StartggRawSlot[];
};

type StartggRawSlot = {
  entrant?: StartggRawEntrant | null;
  standing?: {
    stats?: {
      score?: { value?: number | null; label?: string | null };
    };
  };
  sourceType?: string | null;
  sourceSetId?: number | string | null;
  sourceLabel?: string | null;
};

function normalizeNumber(value: unknown): number | null {
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : null;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function normalizeTimestampMs(value: unknown): number | null {
  const num = normalizeNumber(value);
  if (num === null) {
    return null;
  }
  if (num > 1_000_000_000_000) {
    return num;
  }
  return num * 1000;
}

function pickPhases(raw?: StartggRawEvent): StartggSimState["phases"] {
  const phases = Array.isArray(raw?.phases)
    ? raw?.phases
    : raw?.phases && "nodes" in raw.phases
      ? raw.phases.nodes ?? []
      : [];
  return phases.map((phase, idx) => ({
    id: String(phase.id ?? `phase-${idx + 1}`),
    name: phase.name ?? `Phase ${idx + 1}`,
    bestOf: phase.bestOf ?? 3,
  }));
}

function extractSlippiCode(entrant: StartggRawEntrant): string | null {
  if (entrant.slippiCode) {
    return entrant.slippiCode;
  }
  for (const field of entrant.customFields ?? []) {
    const name = field.name?.toLowerCase() ?? "";
    if (name.includes("slippi") || name.includes("connect")) {
      if (field.value) {
        return field.value;
      }
    }
  }
  for (const participant of entrant.participants ?? []) {
    const playerCode = participant.player?.slippiCode;
    if (playerCode) {
      return playerCode;
    }
    for (const auth of participant.user?.authorizations ?? []) {
      const authType = auth.type?.toLowerCase() ?? "";
      if (authType.includes("slippi") || authType.includes("connect")) {
        if (auth.externalUsername) {
          return auth.externalUsername;
        }
      }
    }
    const tags = [participant.gamerTag, participant.player?.gamerTag];
    for (const tag of tags) {
      if (tag && tag.includes("#")) {
        return tag;
      }
    }
  }
  return null;
}

function mapSetState(value: unknown): string {
  if (typeof value === "string") {
    const lower = value.toLowerCase();
    if (lower.includes("progress")) return "inProgress";
    if (lower.includes("complete")) return "completed";
    if (lower.includes("skip")) return "skipped";
    return "pending";
  }
  const state = normalizeNumber(value);
  switch (state) {
    case 1:
      return "pending";
    case 2:
      return "inProgress";
    case 3:
      return "completed";
    case 4:
      return "skipped";
    case 6:
      return "skipped";
    default:
      return "pending";
  }
}

function resolveRoundLabel(set: StartggRawSet): string {
  if (set.fullRoundText) {
    return set.fullRoundText;
  }
  const round = normalizeNumber(set.round);
  if (round === null) {
    return "Round";
  }
  if (round > 0) {
    return `Winners Round ${round}`;
  }
  if (round < 0) {
    return `Losers Round ${Math.abs(round)}`;
  }
  return "Grand Finals";
}

export function normalizeStartggResponse(raw: StartggRawResponse): StartggSimState | null {
  const event = raw.data?.event;
  if (!event) {
    return null;
  }

  const phases = pickPhases(event);
  const phaseLookup = new Map(phases.map((phase) => [phase.id, phase]));
  const entrantsNodes = event.entrants?.nodes ?? [];
  const entrants = entrantsNodes.map((entrant, index) => {
    const id = normalizeNumber(entrant.id) ?? index + 1;
    const name =
      entrant.name ??
      entrant.participants?.[0]?.gamerTag ??
      entrant.participants?.[0]?.player?.gamerTag ??
      `Entrant ${id}`;
    const seed =
      entrant.seeds?.[0]?.seedNum ??
      entrant.seed ??
      index + 1;
    const slippiCode = extractSlippiCode(entrant) ?? `TEST#${id}`;
    return {
      id,
      name,
      seed,
      slippiCode,
    };
  });

  const entrantsById = new Map(entrants.map((entrant) => [entrant.id, entrant]));
  const setsNodes = event.sets?.nodes ?? [];
  const sets = setsNodes.map((set, index) => {
    const id = normalizeNumber(set.id) ?? index + 1;
    const phaseId = String(set.phaseGroup?.phase?.id ?? phases[0]?.id ?? "phase-1");
    const phaseName = set.phaseGroup?.phase?.name ?? phaseLookup.get(phaseId)?.name ?? "Bracket";
    const round = normalizeNumber(set.round) ?? 0;
    const state = mapSetState(set.state);
    const winnerId = normalizeNumber(set.winnerId);
    const slots = (set.slots ?? []).map((slot) => {
      const entrantId = normalizeNumber(slot.entrant?.id);
      const entrant =
        (entrantId !== null ? entrantsById.get(entrantId) : undefined) ?? null;
      const score = normalizeNumber(slot.standing?.stats?.score?.value);
      const label = slot.standing?.stats?.score?.label ?? null;
      const sourceSetId = normalizeNumber(slot.sourceSetId);
      const sourceType = slot.sourceType ?? null;
      const sourceLabel = slot.sourceLabel ?? null;
      let result: string | null = null;
      if (label && label.toLowerCase().includes("dq")) {
        result = "dq";
      } else if (winnerId !== null && entrantId !== null) {
        result = winnerId === entrantId ? "win" : "loss";
      } else if (state === "completed" && entrantId !== null) {
        result = "loss";
      }
      return {
        entrantId: entrant?.id ?? entrantId,
        entrantName: entrant?.name ?? slot.entrant?.name ?? null,
        slippiCode: entrant?.slippiCode ?? extractSlippiCode(slot.entrant ?? {}),
        seed: entrant?.seed ?? null,
        score: score ?? null,
        result,
        sourceType,
        sourceSetId,
        sourceLabel,
      };
    });
    return {
      id,
      phaseId,
      phaseName,
      round,
      roundLabel: resolveRoundLabel(set),
      bestOf: phaseLookup.get(phaseId)?.bestOf ?? 3,
      state,
      startedAtMs: normalizeTimestampMs(set.startedAt),
      completedAtMs: normalizeTimestampMs(set.completedAt),
      updatedAtMs: normalizeTimestampMs(set.updatedAt) ?? 0,
      winnerId: winnerId ?? null,
      slots,
    };
  });

  const nowMs = raw.extensions?.nowMs ?? Date.now();
  const startedAtMs = raw.extensions?.startedAtMs ?? nowMs;
  const eventLink = raw.extensions?.eventLink ?? null;

  return {
    event: {
      id: String(event.id ?? "event-1"),
      name: event.name ?? "Start.gg Event",
      slug: event.slug ?? "event",
    },
    phases,
    entrants,
    sets,
    startedAtMs,
    nowMs,
    eventLink,
  };
}
