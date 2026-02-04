import type {
  StartggSimEntrant,
  StartggSimSet,
  StartggSimSlot,
  StartggSimState,
  SlippiStream,
} from "./types/overlay";

// ── Key normalization ──────────────────────────────────────────────────

export function normalizeKey(value: string | null | undefined): string {
  return (value ?? "").trim().toLowerCase();
}

export function stripSponsorTag(tag: string | null | undefined): string {
  if (!tag) {
    return "";
  }
  const trimmed = tag.trim();
  if (!trimmed) {
    return "";
  }
  const pipeIndex = trimmed.indexOf("|");
  if (pipeIndex === -1) {
    return trimmed;
  }
  return trimmed.slice(pipeIndex + 1).trim();
}

export function normalizeTagKey(value: string | null | undefined): string {
  const raw = stripSponsorTag(value ?? "");
  const trimmed = raw.trim();
  if (!trimmed) {
    return "";
  }
  const withoutCode = trimmed.split("#")[0] ?? trimmed;
  return withoutCode.trim().toLowerCase();
}

// ── Slot / entrant matching ────────────────────────────────────────────

export function slotMatchesPlayer(
  slot: StartggSimSlot,
  playerCode: string,
  playerTag: string,
): boolean {
  if (playerCode) {
    const slotCode = normalizeKey(slot.slippiCode ?? "");
    if (slotCode && slotCode === playerCode) {
      return true;
    }
  }
  if (playerTag) {
    const slotTag = normalizeTagKey(slot.entrantName ?? "");
    if (slotTag && slotTag === playerTag) {
      return true;
    }
  }
  return false;
}

export function slotHasEntrant(slot: StartggSimSlot): boolean {
  return (
    (slot.entrantId !== null && slot.entrantId !== undefined && slot.entrantId !== 0) ||
    Boolean(slot.entrantName?.trim())
  );
}

export function entrantMatchesSlot(
  entrant: StartggSimEntrant,
  slot: StartggSimSlot,
): boolean {
  if (slot.entrantId !== null && slot.entrantId !== undefined && slot.entrantId === entrant.id) {
    return true;
  }
  const entrantCode = normalizeKey(entrant.slippiCode ?? "");
  const entrantTag = normalizeTagKey(entrant.name ?? "");
  if (!entrantCode && !entrantTag) {
    return false;
  }
  return slotMatchesPlayer(slot, entrantCode, entrantTag);
}

// ── Set helpers ────────────────────────────────────────────────────────

export function setHasBothEntrants(set: StartggSimSet): boolean {
  return set.slots.filter((slot) => slotHasEntrant(slot)).length >= 2;
}

export function setStateRank(state: string): number {
  switch (state) {
    case "inProgress":
      return 0;
    case "pending":
      return 1;
    case "completed":
      return 2;
    case "skipped":
      return 3;
    default:
      return 4;
  }
}

export function isActiveSet(set: StartggSimSet): boolean {
  if (set.state === "completed" || set.state === "skipped") {
    return false;
  }
  return set.slots.some(
    (slot) =>
      (slot.entrantId !== null &&
        slot.entrantId !== undefined &&
        slot.entrantId !== 0) ||
      Boolean(slot.entrantName?.trim()),
  );
}

export function seedValue(seed: number | null | undefined): number {
  if (seed === null || seed === undefined || !Number.isFinite(seed)) {
    return 9999;
  }
  return seed > 0 ? seed : 9999;
}

export function bestSeedForSet(set: StartggSimSet): number {
  const seeds = set.slots
    .map((slot) => seedValue(slot.seed))
    .filter((value) => Number.isFinite(value));
  if (seeds.length === 0) {
    return 9999;
  }
  return Math.min(...seeds);
}

// ── Opponent matching ──────────────────────────────────────────────────

export function findExpectedOpponent(
  state: StartggSimState | null,
  stream: SlippiStream | null | undefined,
  resolveEntrantId: (stream: SlippiStream) => number | null,
): { tag?: string | null; code?: string | null } | null {
  if (!state || !stream) {
    return null;
  }
  const playerCode = normalizeKey(stream.p1Code ?? "");
  const playerTag = normalizeTagKey(stream.p1Tag ?? "");
  const playerEntrantId = resolveEntrantId(stream);
  if (!playerCode && !playerTag && !playerEntrantId) {
    return null;
  }
  const byEntrant = playerEntrantId
    ? state.sets.filter(
        (set) =>
          isActiveSet(set) &&
          set.slots.some((slot) => slot.entrantId === playerEntrantId),
      )
    : [];
  const candidates = (byEntrant.length > 0 ? byEntrant : state.sets)
    .filter((set) => isActiveSet(set))
    .filter((set) =>
      playerEntrantId && byEntrant.length > 0
        ? set.slots.some((slot) => slot.entrantId === playerEntrantId)
        : set.slots.some((slot) => slotMatchesPlayer(slot, playerCode, playerTag)),
    )
    .map((set) => ({ set, rank: setStateRank(set.state) }))
    .sort((a, b) => {
      if (a.rank !== b.rank) {
        return a.rank - b.rank;
      }
      return a.set.id - b.set.id;
    });
  const target = candidates[0]?.set;
  if (!target) {
    return null;
  }
  const opponentSlot = target.slots.find((slot) =>
    playerEntrantId && byEntrant.length > 0
      ? slot.entrantId !== playerEntrantId
      : !slotMatchesPlayer(slot, playerCode, playerTag),
  );
  if (!opponentSlot) {
    return null;
  }
  const tag = opponentSlot.entrantName ?? null;
  const code = opponentSlot.slippiCode ?? null;
  if (!tag && !code) {
    return null;
  }
  return { tag, code };
}

export function findSetForStream(
  state: StartggSimState | null,
  stream: SlippiStream | null | undefined,
  resolveEntrantId: (stream: SlippiStream) => number | null,
): StartggSimSet | null {
  if (!state || !stream) {
    return null;
  }
  const playerCode = normalizeKey(stream.p1Code ?? "");
  const playerTag = normalizeTagKey(stream.p1Tag ?? "");
  const playerEntrantId = resolveEntrantId(stream);
  if (!playerCode && !playerTag && !playerEntrantId) {
    return null;
  }
  const byEntrant = playerEntrantId
    ? state.sets.filter(
        (set) =>
          isActiveSet(set) &&
          set.slots.some((slot) => slot.entrantId === playerEntrantId),
      )
    : [];
  const candidates = (byEntrant.length > 0 ? byEntrant : state.sets)
    .filter((set) => isActiveSet(set))
    .filter((set) =>
      playerEntrantId && byEntrant.length > 0
        ? set.slots.some((slot) => slot.entrantId === playerEntrantId)
        : set.slots.some((slot) => slotMatchesPlayer(slot, playerCode, playerTag)),
    )
    .sort((a, b) => {
      const rankDiff = setStateRank(a.state) - setStateRank(b.state);
      if (rankDiff !== 0) {
        return rankDiff;
      }
      if (a.updatedAtMs !== b.updatedAtMs) {
        return b.updatedAtMs - a.updatedAtMs;
      }
      return a.id - b.id;
    });
  return candidates[0] ?? null;
}

export function opponentMatchesExpected(
  expected: { tag?: string | null; code?: string | null } | null,
  actualTag?: string | null,
  actualCode?: string | null,
): boolean {
  if (!expected) {
    return true;
  }
  const expectedCode = normalizeKey(expected.code ?? "");
  const expectedTag = normalizeTagKey(expected.tag ?? "");
  const actualCodeKey = normalizeKey(actualCode ?? "");
  const actualTagKey = normalizeTagKey(actualTag ?? "");
  if (!actualCodeKey && !actualTagKey) {
    return true;
  }
  if (expectedCode && actualCodeKey && expectedCode === actualCodeKey) {
    return true;
  }
  if (expectedTag && actualTagKey && expectedTag === actualTagKey) {
    return true;
  }
  if (!expectedCode && !expectedTag) {
    return true;
  }
  return false;
}

// ── Bracket display helpers ────────────────────────────────────────────

export function columnLabel(round: number, isLosers: boolean): string {
  if (round === 0) {
    return "GF";
  }
  const label = Math.abs(round);
  return isLosers ? `L${label}` : `W${label}`;
}

export function formatSetState(state: string): string {
  switch (state) {
    case "inProgress":
      return "Live";
    case "pending":
      return "Not started";
    case "completed":
      return "Complete";
    case "skipped":
      return "Skipped";
    default:
      return state;
  }
}

export function isReplayFilename(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.endsWith(".slp") || lower.endsWith(".slippi");
}

// ── Slot label resolution ──────────────────────────────────────────────

export function resolveSlotLabel(
  slot: StartggSimSlot,
  setsById: Map<number, StartggSimSet>,
  visitedSourceIds: Set<number> = new Set(),
): string {
  const directName = stripSponsorTag(slot.entrantName);
  if (directName) {
    return directName;
  }

  if (slot.sourceSetId !== null && slot.sourceSetId !== undefined && setsById.size > 0) {
    if (!visitedSourceIds.has(slot.sourceSetId)) {
      const nextVisited = new Set(visitedSourceIds);
      nextVisited.add(slot.sourceSetId);
      const sourceSet = setsById.get(slot.sourceSetId);
      if (sourceSet) {
        const expandCandidates = (label: string): string[] => {
          if (!label.includes(" or ")) {
            return [label];
          }
          return label
            .split(" or ")
            .map((part) => part.trim())
            .filter(Boolean);
        };
        const labels = sourceSet.slots
          .map((sourceSlot) => resolveSlotLabel(sourceSlot, setsById, nextVisited))
          .filter((name) => Boolean(name));
        let hasUnknown = false;
        const candidates: string[] = [];
        for (const label of labels) {
          const trimmed = label.trim();
          if (!trimmed) {
            hasUnknown = true;
            continue;
          }
          if (trimmed === "Bye") {
            continue;
          }
          if (trimmed === "Awaiting match" || trimmed === "TBD") {
            hasUnknown = true;
            continue;
          }
          candidates.push(...expandCandidates(trimmed));
        }
        const unique = Array.from(new Set(candidates));
        if (unique.length > 2) {
          return "TBD";
        }
        if (unique.length === 2) {
          return `${unique[0]} or ${unique[1]}`;
        }
        if (unique.length === 1) {
          return hasUnknown ? `${unique[0]} or TBD` : unique[0];
        }
        if (hasUnknown) {
          return "TBD";
        }
      }
    }
  }

  if (slot.sourceType === "empty") {
    return "Bye";
  }

  if (slot.sourceLabel) {
    return slot.sourceLabel;
  }

  return "Awaiting match";
}
