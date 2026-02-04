import { useState, useRef, useMemo, useEffect, type DragEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppConfig,
  AssignStreamResult,
  Setup,
  SlippiStream,
  SlippiWindowInfo,
  StartggSimEntrant,
  StartggSimState,
  ReplayStreamUpdate,
} from "../types/overlay";
import {
  stripSponsorTag,
  normalizeKey,
  normalizeTagKey,
  isActiveSet,
  setStateRank,
  bestSeedForSet,
  findSetForStream,
} from "../tournamentUtils";

export type UseStreamsReturn = {
  streams: SlippiStream[];
  setStreams: React.Dispatch<React.SetStateAction<SlippiStream[]>>;
  streamsStatus: string;
  streamEntrantLinks: Record<string, number>;
  draggedStreamId: string | null;
  topStatus: string;
  setTopStatus: React.Dispatch<React.SetStateAction<string>>;
  windowInfo: SlippiWindowInfo | null;
  windowStatus: string;
  launchStatus: string;
  slippiRefreshStatus: string;
  spoofStatus: string;
  watchStatus: string;
  slippiIsOpen: boolean;
  liveStreamIds: Set<string>;
  linkedStreamByEntrantId: Map<number, SlippiStream>;
  entrantLookup: Map<number, StartggSimEntrant>;
  resolveStreamEntrantId: (stream: SlippiStream | null | undefined) => number | null;
  refreshStreams: () => Promise<void>;
  linkStreamToEntrant: (streamId: string, entrantId: number) => void;
  unlinkStream: (streamId: string) => void;
  handleStreamDragStart: (event: DragEvent, stream: SlippiStream) => void;
  handleStreamDragEnd: () => void;
  handleAttendeeDragOver: (event: DragEvent) => void;
  handleAttendeeDrop: (event: DragEvent, entrant: StartggSimState["entrants"][number]) => void;
  applyStreamAssignment: (
    stream: SlippiStream,
    setupId: number,
    options?: { silent?: boolean; launch?: boolean; source?: "auto" | "manual" | "system" },
  ) => Promise<void>;
  watchStream: (
    stream: SlippiStream,
    setupOverride?: number,
    options?: { launch?: boolean; source?: "auto" | "manual" | "system" },
  ) => Promise<void>;
  launchSetupStream: (setup: Setup) => Promise<void>;
  applyAutoStreamAssignments: () => Promise<void>;
  rebuildAutoStreamAssignments: () => Promise<void>;
  handleSetupSelect: (stream: SlippiStream, value: string) => Promise<void>;
  launchSlippi: () => Promise<void>;
  refreshSlippiThenScan: () => Promise<void>;
  spoofLiveGames: () => Promise<void>;
  getStreamSetupId: (streamId: string) => number | null;
  openBracketWindow: () => Promise<void>;
  refreshStreamsRef: React.MutableRefObject<(() => Promise<void>) | null>;
};

export function useStreams(deps: {
  isBracketView: boolean;
  config: AppConfig;
  setups: Setup[];
  setSetups: React.Dispatch<React.SetStateAction<Setup[]>>;
  currentStartggState: StartggSimState | null;
  autoManagedSetupIds: React.MutableRefObject<Set<number>>;
  streamSetupSelections: Record<string, number>;
  setStreamSetupSelections: React.Dispatch<React.SetStateAction<Record<string, number>>>;
  clearSetupAssignment: (id: number, streamId?: string, options?: { silent?: boolean; stop?: boolean }) => Promise<boolean>;
  setEphemeralSetupStatus: (message: string, timeoutMs?: number) => void;
  setPersistentSetupStatus: (message: string) => void;
  refreshTestStartggState: () => Promise<StartggSimState | null>;
}): UseStreamsReturn {
  const {
    isBracketView, config, setups, setSetups, currentStartggState,
    autoManagedSetupIds, streamSetupSelections, setStreamSetupSelections,
    clearSetupAssignment, setEphemeralSetupStatus, setPersistentSetupStatus,
    refreshTestStartggState,
  } = deps;

  const [streams, setStreams] = useState<SlippiStream[]>([]);
  const [streamsStatus, setStreamsStatus] = useState("Scan for live streams.");
  const [streamEntrantLinks, setStreamEntrantLinks] = useState<Record<string, number>>({});
  const [draggedStreamId, setDraggedStreamId] = useState<string | null>(null);
  const [topStatus, setTopStatus] = useState("");
  const [windowInfo, setWindowInfo] = useState<SlippiWindowInfo | null>(null);
  const [windowStatus, setWindowStatus] = useState("");
  const [launchStatus, setLaunchStatus] = useState("");
  const [slippiRefreshStatus, setSlippiRefreshStatus] = useState("");
  const [spoofStatus, setSpoofStatus] = useState("");
  const [watchStatus, setWatchStatus] = useState("");
  const refreshInFlight = useRef(false);
  const autoStreamInFlight = useRef(false);
  const refreshStreamsRef = useRef<(() => Promise<void>) | null>(null);

  const slippiIsOpen = Boolean(windowInfo);
  const liveStreamIds = useMemo(() => new Set(streams.map((s) => s.id)), [streams]);

  const entrantLookup = useMemo(() => {
    if (!currentStartggState) return new Map<number, StartggSimEntrant>();
    return new Map(currentStartggState.entrants.map((e) => [e.id, e]));
  }, [currentStartggState]);

  const linkedStreamByEntrantId = useMemo(() => {
    const map = new Map<number, SlippiStream>();
    if (streams.length === 0) return map;
    const streamById = new Map(streams.map((s) => [s.id, s]));
    for (const [streamId, entrantId] of Object.entries(streamEntrantLinks)) {
      const stream = streamById.get(streamId);
      if (stream && Number.isFinite(entrantId)) {
        map.set(entrantId, stream);
      }
    }
    return map;
  }, [streamEntrantLinks, streams]);

  function resolveStreamEntrantId(stream: SlippiStream | null | undefined): number | null {
    if (!stream) return null;
    if (stream.startggEntrantId) return stream.startggEntrantId;
    const linked = streamEntrantLinks[stream.id];
    if (!linked || !Number.isFinite(linked)) return null;
    return linked;
  }

  // Clean up entrant links for offline streams
  useEffect(() => {
    setStreamEntrantLinks((prev) => {
      if (streams.length === 0) {
        return Object.keys(prev).length > 0 ? {} : prev;
      }
      const liveIds = new Set(streams.map((s) => s.id));
      let changed = false;
      const next: Record<string, number> = {};
      for (const [streamId, entrantId] of Object.entries(prev)) {
        if (liveIds.has(streamId)) {
          next[streamId] = entrantId;
        } else {
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [streams]);

  async function refreshStreams() {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    setWindowStatus("");
    try {
      try {
        const win = await invoke<SlippiWindowInfo | null>("find_slippi_launcher_window");
        if (win) {
          setWindowInfo(win);
        } else {
          setWindowInfo(null);
          setWindowStatus("Slippi Launcher window not found.");
          setStreams([]);
          setStreamsStatus("Slippi Launcher window not found. Launch it, then refresh.");
          return;
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
        setWindowInfo(null);
        setWindowStatus(`Window search failed: ${msg}`);
        setStreams([]);
        setStreamsStatus("Could not find Slippi Launcher. Launch it, then refresh.");
        return;
      }
      setStreamsStatus("Scanning for streams…");
      try {
        const res = await invoke<SlippiStream[]>("scan_slippi_streams");
        setStreams(res);
        setStreamsStatus(res.length === 0 ? "" : `Found ${res.length} stream(s).`);
        await cleanupMissingAssignments(res);
        if (config.testMode) {
          await syncTestAssignments(res);
          if (config.autoStream) {
            await refreshTestStartggState();
          }
        }
      } catch {
        setStreams([]);
        setStreamsStatus("Could not scan streams. Make sure Slippi is running, then refresh.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setStreams([]);
      setStreamsStatus(`Scan failed: ${msg}`);
    } finally {
      refreshInFlight.current = false;
    }
  }

  useEffect(() => {
    refreshStreamsRef.current = refreshStreams;
  }, [refreshStreams]);

  async function cleanupMissingAssignments(nextStreams: SlippiStream[]) {
    if (setups.length === 0) return;
    const activeIds = new Set(nextStreams.map((s) => s.id));
    const missing = setups.filter((s) => s.assignedStream && !activeIds.has(s.assignedStream.id));
    if (missing.length === 0) return;
    setEphemeralSetupStatus(
      `${missing.length} setup${missing.length === 1 ? "" : "s"} offline.`,
    );
  }

  function linkStreamToEntrant(streamId: string, entrantId: number) {
    setStreamEntrantLinks((prev) => ({ ...prev, [streamId]: entrantId }));
    const assignedSetup = setups.find((s) => s.assignedStream?.id === streamId);
    const stream = streams.find((s) => s.id === streamId);
    if (assignedSetup && stream) {
      void applyStreamAssignment(
        { ...stream, startggEntrantId: entrantId },
        assignedSetup.id,
        { silent: true, launch: false, source: "manual" },
      );
    }
  }

  function unlinkStream(streamId: string) {
    setStreamEntrantLinks((prev) => {
      if (!(streamId in prev)) return prev;
      const next = { ...prev };
      delete next[streamId];
      return next;
    });
  }

  function handleStreamDragStart(event: DragEvent, stream: SlippiStream) {
    event.dataTransfer.setData("application/x-nmst-stream-id", stream.id);
    event.dataTransfer.setData("text/plain", stream.id);
    event.dataTransfer.effectAllowed = "link";
    setDraggedStreamId(stream.id);
  }

  function handleStreamDragEnd() {
    setDraggedStreamId(null);
  }

  function handleAttendeeDragOver(event: DragEvent) {
    if (draggedStreamId) event.preventDefault();
  }

  function handleAttendeeDrop(event: DragEvent, entrant: StartggSimState["entrants"][number]) {
    event.preventDefault();
    const streamId =
      event.dataTransfer.getData("application/x-nmst-stream-id") ||
      event.dataTransfer.getData("text/plain");
    if (!streamId) return;
    linkStreamToEntrant(streamId, entrant.id);
    setDraggedStreamId(null);
  }

  async function applyStreamAssignment(
    stream: SlippiStream,
    setupId: number,
    options: { silent?: boolean; launch?: boolean; source?: "auto" | "manual" | "system" } = {},
  ) {
    const silent = options.silent ?? false;
    const launch = options.launch ?? true;
    const source = options.source ?? "system";
    const linkedEntrantId = stream.startggEntrantId ?? streamEntrantLinks[stream.id];
    const streamPayload =
      linkedEntrantId && Number.isFinite(linkedEntrantId)
        ? { ...stream, startggEntrantId: linkedEntrantId }
        : stream;
    const label = stripSponsorTag(stream.p1Tag) || stream.p1Code || stream.windowTitle || stream.id;
    if (!silent) {
      setWatchStatus(`Assigning "${label}" to setup ${setupId}…`);
    }
    try {
      const result = await invoke<AssignStreamResult>("assign_stream_to_setup", {
        setupId,
        stream: streamPayload,
        launch,
      });
      if (source === "auto") {
        autoManagedSetupIds.current.add(setupId);
      } else if (source === "manual") {
        autoManagedSetupIds.current.delete(setupId);
      }
      setSetups(result.setups);
      const nextSelections: Record<string, number> = {};
      for (const setup of result.setups) {
        if (setup.assignedStream) {
          nextSelections[setup.assignedStream.id] = setup.id;
        }
      }
      setStreamSetupSelections(nextSelections);
      if (!silent) {
        if (stream.isPlaying === true) {
          setWatchStatus(`Assigned "${label}" to setup ${setupId}.`);
        } else {
          setWatchStatus(`Assigned "${label}" to setup ${setupId}. Not playing yet.`);
        }
        if (result.warning) {
          setWatchStatus((prev) => `${prev} ${result.warning}`.trim());
        }
      } else if (result.warning) {
        setWatchStatus(result.warning);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setWatchStatus(`Assign failed: ${msg}`);
    }
  }

  function getStreamSetupId(streamId: string) {
    if (streamSetupSelections[streamId] !== undefined) return streamSetupSelections[streamId];
    return null;
  }

  async function watchStream(
    stream: SlippiStream,
    setupOverride?: number,
    options: { launch?: boolean; source?: "auto" | "manual" | "system" } = {},
  ) {
    const setupId = setupOverride ?? getStreamSetupId(stream.id);
    if (setupId === null) {
      setWatchStatus("Select a setup first.");
      return;
    }
    const launch = options.launch ?? true;
    await applyStreamAssignment(stream, setupId, { launch, source: options.source });
  }

  async function launchSetupStream(setup: Setup) {
    if (!setup.assignedStream) return;
    await applyStreamAssignment(setup.assignedStream, setup.id, { launch: true, source: "system" });
  }

  async function applyAutoStreamAssignments() {
    if (isBracketView || !config.autoStream) return;
    if (autoStreamInFlight.current) return;
    if (!currentStartggState) return;
    autoStreamInFlight.current = true;
    try {
      const resolveEntrantId = (s: SlippiStream) => resolveStreamEntrantId(s);
      const activeSets = currentStartggState.sets.filter((set) => isActiveSet(set));
      const activeSetIds = new Set(activeSets.map((set) => set.id));
      const lockedSetupIds = new Set<number>();
      const usedSetIds = new Map<number, number>();
      const usedStreamIds = new Set<string>();
      const setupStreamIds = new Map<number, string | null>();

      for (const setup of setups) {
        const assigned = setup.assignedStream;
        setupStreamIds.set(setup.id, assigned?.id ?? null);
        if (!assigned) continue;
        usedStreamIds.add(assigned.id);
        const isAutoManaged = autoManagedSetupIds.current.has(setup.id);
        if (!isAutoManaged) {
          lockedSetupIds.add(setup.id);
          const set = findSetForStream(currentStartggState, assigned, resolveEntrantId);
          if (set && activeSetIds.has(set.id)) {
            usedSetIds.set(set.id, setup.id);
          }
          continue;
        }
        const set = findSetForStream(currentStartggState, assigned, resolveEntrantId);
        if (set && activeSetIds.has(set.id)) {
          lockedSetupIds.add(setup.id);
          usedSetIds.set(set.id, setup.id);
        }
      }

      const candidates = streams
        .map((stream) => {
          const set = findSetForStream(currentStartggState, stream, resolveEntrantId);
          if (!set || !activeSetIds.has(set.id)) return null;
          return { stream, set, seed: bestSeedForSet(set) };
        })
        .filter((item): item is NonNullable<typeof item> => Boolean(item))
        .sort((a, b) => {
          if (a.seed !== b.seed) return a.seed - b.seed;
          const rankDiff = setStateRank(a.set.state) - setStateRank(b.set.state);
          if (rankDiff !== 0) return rankDiff;
          if (a.set.updatedAtMs !== b.set.updatedAtMs) return b.set.updatedAtMs - a.set.updatedAtMs;
          return a.set.id - b.set.id;
        });

      const availableSetups = setups
        .map((s) => s.id)
        .filter((id) => !lockedSetupIds.has(id))
        .sort((a, b) => a - b);
      const pendingAssignments: Array<{ setupId: number; stream: SlippiStream }> = [];

      for (const candidate of candidates) {
        if (usedSetIds.has(candidate.set.id)) continue;
        if (usedStreamIds.has(candidate.stream.id)) continue;
        const setupId = availableSetups.shift();
        if (setupId === undefined) break;
        pendingAssignments.push({ setupId, stream: candidate.stream });
        usedSetIds.set(candidate.set.id, setupId);
        usedStreamIds.add(candidate.stream.id);
      }

      const pendingSetupIds = new Set(pendingAssignments.map((e) => e.setupId));
      const clearTargets = setups.filter(
        (s) => s.assignedStream && !lockedSetupIds.has(s.id) && !pendingSetupIds.has(s.id),
      );

      for (const setup of clearTargets) {
        await clearSetupAssignment(setup.id, setup.assignedStream?.id, { silent: true, stop: false });
      }

      for (const assignment of pendingAssignments) {
        const existingStreamId = setupStreamIds.get(assignment.setupId);
        if (existingStreamId === assignment.stream.id) continue;
        await applyStreamAssignment(assignment.stream, assignment.setupId, {
          silent: true,
          launch: false,
          source: "auto",
        });
      }
    } finally {
      autoStreamInFlight.current = false;
    }
  }

  async function rebuildAutoStreamAssignments() {
    if (!config.autoStream) {
      setEphemeralSetupStatus("Auto stream is disabled.");
      return;
    }
    if (autoStreamInFlight.current) return;
    setPersistentSetupStatus("Rebuilding auto stream…");
    autoManagedSetupIds.current.clear();
    for (const setup of setups) {
      if (!setup.assignedStream) continue;
      await clearSetupAssignment(setup.id, setup.assignedStream.id, { silent: true, stop: true });
    }
    if (config.testMode) {
      await refreshTestStartggState();
    }
    await applyAutoStreamAssignments();
    setEphemeralSetupStatus("Auto stream rebuilt.");
  }

  async function handleSetupSelect(stream: SlippiStream, value: string) {
    setStreamSetupSelections((prev) => {
      const next = { ...prev };
      if (!value) {
        delete next[stream.id];
      } else {
        next[stream.id] = Number(value);
      }
      return next;
    });
    if (value) {
      await watchStream(stream, Number(value), { launch: false, source: "manual" });
    } else {
      setWatchStatus("");
    }
  }

  async function syncTestAssignments(nextStreams: SlippiStream[]) {
    if (!config.testMode || setups.length === 0) return;
    const streamById = new Map(nextStreams.map((s) => [s.id, s]));
    for (const setup of setups) {
      const assignedId = setup.assignedStream?.id;
      if (!assignedId) continue;
      const stream = streamById.get(assignedId);
      if (!stream) continue;
      await applyStreamAssignment(stream, setup.id, { silent: true, launch: false, source: "system" });
    }
  }

  async function launchSlippi() {
    const relaunch = slippiIsOpen;
    setLaunchStatus(relaunch ? "Relaunching Slippi Launcher…" : "Launching Slippi Launcher…");
    setWindowStatus("");
    try {
      if (relaunch) {
        await invoke("relaunch_slippi_app");
        setLaunchStatus("Slippi relaunched. Give it a moment, then hit Refresh.");
      } else {
        await invoke("launch_slippi_app");
        setLaunchStatus("Slippi launch requested. Give it a moment, then hit Refresh.");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setLaunchStatus(`${relaunch ? "Relaunch" : "Launch"} failed: ${msg}`);
    }
  }

  async function refreshSlippiThenScan() {
    setSlippiRefreshStatus("Refreshing Slippi Launcher…");
    let clicked = false;
    try {
      await invoke("refresh_slippi_launcher");
      setSlippiRefreshStatus("Slippi Launcher refresh clicked. Scanning streams…");
      clicked = true;
      await new Promise((resolve) => setTimeout(resolve, 600));
    } catch {
      setSlippiRefreshStatus("Slippi refresh failed. Make sure Slippi is running.");
    }
    await refreshStreams();
    if (clicked) {
      setSlippiRefreshStatus("");
    }
  }

  async function spoofLiveGames() {
    if (!config.testMode) {
      setSpoofStatus("Enable test mode in settings before spoofing streams.");
      return;
    }
    if (!config.spectateFolderPath.trim()) {
      setSpoofStatus("Set a spectate folder path in settings before spoofing streams.");
      return;
    }
    setSpoofStatus("Starting spoof streams…");
    try {
      const res = await invoke<SlippiStream[]>("spoof_live_games");
      setStreams(res);
      setStreamsStatus(
        res.length === 0 ? "No spoof streams created." : `Spoofing ${res.length} stream(s).`,
      );
      await syncTestAssignments(res);
      setSpoofStatus("Spoofing started. Replays are writing to the spectate folder.");
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setSpoofStatus(`Spoofing failed: ${msg}`);
    }
  }

  async function openBracketWindow() {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    setTopStatus("");
    try {
      const existing = await WebviewWindow.getByLabel("spoof-bracket");
      if (existing) {
        await existing.setFocus();
        return;
      }
      const webview = new WebviewWindow("spoof-bracket", {
        url: "/?view=bracket",
        title: "Test Bracket Controller",
        width: 1100,
        height: 760,
        resizable: true,
      });
      webview.once("tauri://error", (event) => {
        const payload = typeof event.payload === "string" ? event.payload : JSON.stringify(event.payload);
        setTopStatus(`Open bracket window failed: ${payload}`);
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setTopStatus(`Open bracket window failed: ${msg}`);
    }
  }

  // Auto-stream assignments
  useEffect(() => {
    if (isBracketView || !config.autoStream) return;
    applyAutoStreamAssignments();
  }, [isBracketView, config.autoStream, currentStartggState, streams, setups]);

  // Spoof replay progress listener for non-bracket view
  useEffect(() => {
    if (isBracketView || !config.testMode) return;
    let unlisten: UnlistenFn | null = null;
    listen<ReplayStreamUpdate>("spoof-replay-progress", (event) => {
      const payload = event.payload;
      const shouldRefresh =
        payload?.type === "start" || payload?.type === "complete" || payload?.type === "error";
      if (shouldRefresh) {
        refreshStreamsRef.current?.();
      }
    })
      .then((fn) => { unlisten = fn; })
      .catch(() => { unlisten = null; });
    return () => {
      if (unlisten) unlisten();
    };
  }, [isBracketView, config.testMode]);

  return {
    streams,
    setStreams,
    streamsStatus,
    streamEntrantLinks,
    draggedStreamId,
    topStatus,
    setTopStatus,
    windowInfo,
    windowStatus,
    launchStatus,
    slippiRefreshStatus,
    spoofStatus,
    watchStatus,
    slippiIsOpen,
    liveStreamIds,
    linkedStreamByEntrantId,
    entrantLookup,
    resolveStreamEntrantId,
    refreshStreams,
    linkStreamToEntrant,
    unlinkStream,
    handleStreamDragStart,
    handleStreamDragEnd,
    handleAttendeeDragOver,
    handleAttendeeDrop,
    applyStreamAssignment,
    watchStream,
    launchSetupStream,
    applyAutoStreamAssignments,
    rebuildAutoStreamAssignments,
    handleSetupSelect,
    launchSlippi,
    refreshSlippiThenScan,
    spoofLiveGames,
    getStreamSetupId,
    openBracketWindow,
    refreshStreamsRef,
  };
}
