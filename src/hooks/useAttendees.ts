import { useMemo } from "react";
import type {
  SlippiStream,
  StartggSimEntrant,
  StartggSimState,
} from "../types/overlay";
import {
  normalizeKey,
  normalizeTagKey,
  stripSponsorTag,
  seedValue,
  entrantMatchesSlot,
} from "../tournamentUtils";

type AttendeeStartggState = "active" | "waiting" | "out";
type AttendeeStatus = { state: AttendeeStartggState; label: string };

export type UseAttendeesReturn = {
  attendeeStatusMap: Map<number, AttendeeStatus>;
  attendeeList: StartggSimEntrant[];
  attendeePlayingMap: Map<number, boolean>;
  attendeeBroadcastMap: Map<number, boolean>;
};

export function useAttendees(deps: {
  currentStartggState: StartggSimState | null;
  streams: SlippiStream[];
  linkedStreamByEntrantId: Map<number, SlippiStream>;
}): UseAttendeesReturn {
  const { currentStartggState, streams, linkedStreamByEntrantId } = deps;

  const attendeeStatusMap = useMemo(() => {
    const map = new Map<number, AttendeeStatus>();
    if (!currentStartggState) return map;
    const labels: Record<AttendeeStartggState, string> = {
      active: "Active",
      waiting: "Waiting",
      out: "Out",
    };
    for (const entrant of currentStartggState.entrants) {
      const entrantSets = currentStartggState.sets.filter((set) =>
        set.slots.some((slot) => entrantMatchesSlot(entrant, slot)),
      );
      const hasActive = entrantSets.some((set) => set.state === "inProgress");
      const hasPending = entrantSets.some((set) => set.state === "pending");
      let latestCompleted: { set: StartggSimState["sets"][number]; time: number } | null = null;
      for (const set of entrantSets) {
        if (set.state === "completed" || set.state === "skipped") {
          const stamp = set.completedAtMs ?? set.updatedAtMs ?? 0;
          if (!latestCompleted || stamp > latestCompleted.time) {
            latestCompleted = { set, time: stamp };
          }
        }
      }
      let state: AttendeeStartggState = "waiting";
      if (hasActive) {
        state = "active";
      } else if (!hasPending && latestCompleted) {
        const slot = latestCompleted.set.slots.find((item) => entrantMatchesSlot(entrant, item));
        if (slot?.result === "loss" || slot?.result === "dq") {
          state = "out";
        }
      }
      map.set(entrant.id, { state, label: labels[state] });
    }
    return map;
  }, [currentStartggState]);

  const attendeeList = useMemo(() => {
    if (!currentStartggState) return [];
    return [...currentStartggState.entrants].sort((a, b) => {
      const stateA = attendeeStatusMap.get(a.id)?.state ?? "waiting";
      const stateB = attendeeStatusMap.get(b.id)?.state ?? "waiting";
      if (stateA === "out" && stateB !== "out") return 1;
      if (stateB === "out" && stateA !== "out") return -1;
      const seedA = seedValue(a.seed);
      const seedB = seedValue(b.seed);
      if (seedA !== seedB) return seedA - seedB;
      const nameA = stripSponsorTag(a.name).toLowerCase();
      const nameB = stripSponsorTag(b.name).toLowerCase();
      if (nameA !== nameB) return nameA.localeCompare(nameB);
      return a.id - b.id;
    });
  }, [currentStartggState, attendeeStatusMap]);

  const attendeePlayingMap = useMemo(() => {
    const map = new Map<number, boolean>();
    if (!currentStartggState || streams.length === 0) return map;
    const playingStreams = streams.filter((s) => s.isPlaying === true);
    if (playingStreams.length === 0) {
      for (const entrant of currentStartggState.entrants) {
        const linked = linkedStreamByEntrantId.get(entrant.id);
        if (linked) map.set(entrant.id, false);
      }
      return map;
    }
    for (const entrant of currentStartggState.entrants) {
      const linked = linkedStreamByEntrantId.get(entrant.id);
      if (linked) {
        map.set(entrant.id, linked.isPlaying === true);
        continue;
      }
      const entrantCode = normalizeKey(entrant.slippiCode ?? "");
      const entrantTag = normalizeTagKey(entrant.name ?? "");
      const isPlaying = playingStreams.some((stream) => {
        const streamCodes = [normalizeKey(stream.p1Code ?? ""), normalizeKey(stream.p2Code ?? "")];
        const streamTags = [normalizeTagKey(stream.p1Tag ?? ""), normalizeTagKey(stream.p2Tag ?? "")];
        if (entrantCode && streamCodes.includes(entrantCode)) return true;
        if (entrantTag && streamTags.includes(entrantTag)) return true;
        return false;
      });
      map.set(entrant.id, isPlaying);
    }
    return map;
  }, [currentStartggState, streams, linkedStreamByEntrantId]);

  const attendeeBroadcastMap = useMemo(() => {
    const map = new Map<number, boolean>();
    if (!currentStartggState || streams.length === 0) return map;
    for (const entrant of currentStartggState.entrants) {
      const linked = linkedStreamByEntrantId.get(entrant.id);
      if (linked) {
        map.set(entrant.id, true);
        continue;
      }
      const entrantCode = normalizeKey(entrant.slippiCode ?? "");
      const entrantTag = normalizeTagKey(entrant.name ?? "");
      const isBroadcasting = streams.some((stream) => {
        const streamCode = normalizeKey(stream.p1Code ?? "");
        const streamTag = normalizeTagKey(stream.p1Tag ?? "");
        if (entrantCode && streamCode && entrantCode === streamCode) return true;
        if (entrantTag && streamTag && entrantTag === streamTag) return true;
        return false;
      });
      map.set(entrant.id, isBroadcasting);
    }
    return map;
  }, [currentStartggState, streams, linkedStreamByEntrantId]);

  return {
    attendeeStatusMap,
    attendeeList,
    attendeePlayingMap,
    attendeeBroadcastMap,
  };
}
