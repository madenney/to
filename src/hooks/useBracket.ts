import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type MouseEvent,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AppConfig,
  BroadcastPlayerSelection,
  ReplayStreamUpdate,
  SpoofReplayResult,
  StartggSimState,
} from "../types/overlay";
import { normalizeStartggResponse } from "../startggAdapter";
import {
  isReplayFilename,
  resolveSlotLabel,
  stripSponsorTag,
  formatSetState,
  columnLabel,
} from "../tournamentUtils";

// ── Constants ────────────────────────────────────────────────────────────

export const DEFAULT_TEST_BRACKET_PATH = "test_brackets/test_bracket_2.json";
export const BRACKET_ZOOM_MIN = 0.5;
export const BRACKET_ZOOM_MAX = 1.5;
export const BRACKET_ZOOM_STEP = 0.05;
export const BRACKET_SET_HEIGHT = 88;

// ── Types ────────────────────────────────────────────────────────────────

export type UseBracketDeps = {
  isBracketView: boolean;
  config: AppConfig;
  selectedBracketPath: string;
  setTestStartggState: React.Dispatch<React.SetStateAction<StartggSimState | null>>;
  loadBracketConfigs: () => Promise<void>;
};

export type UseBracketReturn = {
  bracketState: StartggSimState | null;
  bracketStatus: string;
  bracketSettingsOpen: boolean;
  bracketSetDetailsId: number | null;
  bracketSetReplayPaths: string[];
  bracketSetDetailsStatus: string;
  bracketSetActionStatus: string;
  bracketZoom: number;
  setBracketZoom: React.Dispatch<React.SetStateAction<number>>;
  bracketDropTarget: number | null;
  isDraggingReplays: boolean;
  recentDropSetId: number | null;
  isBracketPanning: boolean;
  replaySetIds: number[];
  replayStreamUpdate: ReplayStreamUpdate | null;
  replayStreamStartedAt: number | null;
  broadcastSelections: Record<number, boolean>;
  bracketScrollRef: React.RefObject<HTMLDivElement | null>;
  bracketSetDetails: StartggSimState["sets"][number] | null;
  bracketSetsById: Map<number, StartggSimState["sets"][number]>;
  bracketSetDetailsJson: string;
  bracketSetIsPending: boolean;
  bracketSetIsCompleted: boolean;
  bracketRounds: {
    winnersRounds: [number, StartggSimState["sets"]][];
    losersRounds: [number, StartggSimState["sets"]][];
    winnersBase: number;
    losersBase: number;
  } | null;
  broadcastEntrants: StartggSimState["entrants"];
  broadcastActiveCount: number;
  replaySet: Set<number>;
  isRefreshing: boolean;
  setBracketStatus: (status: string) => void;
  applyNormalizedState: (next: StartggSimState) => void;
  refreshBracketState: () => Promise<void>;
  resetBracketState: (pathOverride?: string, autoCompleteOverride?: boolean) => Promise<void>;
  completeBracket: () => Promise<void>;
  streamBracketReplay: (setId: number) => Promise<void>;
  streamBracketReplayGame: (
    setId: number,
    replayPath: string,
    replayIndex: number,
    replayTotal: number,
  ) => Promise<void>;
  applyStartggUpdate: (action: () => Promise<unknown>, successMessage?: string) => Promise<void>;
  startMatchForSet: (setId: number) => Promise<void>;
  stepBracketSet: (setId: number) => Promise<void>;
  finalizeSetFromReference: (setId: number) => Promise<void>;
  resetSet: (setId: number) => Promise<void>;
  applyReplayResult: (
    setId: number,
    replayPath?: string | null,
    options?: { successMessage?: string },
  ) => Promise<void>;
  openBracketSetDetails: (setId: number) => Promise<void>;
  closeBracketSetDetails: () => void;
  openBracketSettings: () => void;
  closeBracketSettings: () => void;
  toggleBroadcast: (entrantId: number) => void;
  loadReplaySets: (configPath: string) => Promise<void>;
  saveReplayPathsToSet: (
    setId: number,
    replayPaths: string[],
    options?: { ignored?: number; missingPath?: boolean },
  ) => Promise<void>;
  flashDropSuccess: (setId: number) => void;
  hasFileDrag: (event: DragEvent<HTMLElement>) => boolean;
  resetBracketDragState: () => void;
  handleBracketDragEnter: (event: DragEvent<HTMLDivElement>) => void;
  handleBracketDragLeave: (event: DragEvent<HTMLDivElement>) => void;
  handleSetDragOver: (event: DragEvent<HTMLDivElement>, setId: number) => void;
  handleSetDragLeave: (setId: number) => void;
  handleSetDrop: (event: DragEvent<HTMLDivElement>, setId: number) => Promise<void>;
  handleBracketPanStart: (event: MouseEvent<HTMLDivElement>) => void;
  handleBracketPanStop: () => void;
  startBracketPan: (event: {
    button: number;
    clientX: number;
    target: EventTarget | null;
    preventDefault?: () => void;
    stopPropagation?: () => void;
  }) => void;
  resolveBracketScrollTarget: (target: EventTarget | null) => HTMLElement | null;
  getSetIdFromPoint: (x: number, y: number) => number | null;
  updateDropTargetFromPosition: (position: { x: number; y: number }) => void;
  columnHeight: (baseCount: number) => string | undefined;
  isReplayInProgress: (setId: number) => boolean;
  openEventLink: (url: string) => Promise<void>;
  resolveSlotLabel: (
    slot: StartggSimState["sets"][number]["slots"][number],
    visitedSourceIds?: Set<number>,
  ) => string;
  formatSetState: (state: string) => string;
  columnLabel: (round: number, isLosers: boolean) => string;
  cancelReplayStream: () => Promise<void>;
};

// ── Hook ─────────────────────────────────────────────────────────────────

export function useBracket(deps: UseBracketDeps): UseBracketReturn {
  const {
    isBracketView,
    config,
    selectedBracketPath,
    setTestStartggState,
    loadBracketConfigs,
  } = deps;

  // ── State ────────────────────────────────────────────────────────────

  const [bracketState, setBracketState] = useState<StartggSimState | null>(null);
  const [bracketStatus, setBracketStatus] = useState<string>("");
  const [bracketSettingsOpen, setBracketSettingsOpen] = useState<boolean>(false);
  const [bracketSetDetailsId, setBracketSetDetailsId] = useState<number | null>(null);
  const [bracketSetReplayPaths, setBracketSetReplayPaths] = useState<string[]>([]);
  const [bracketSetDetailsStatus, setBracketSetDetailsStatus] = useState<string>("");
  const [bracketSetActionStatus, setBracketSetActionStatus] = useState<string>("");
  const [bracketZoom, setBracketZoom] = useState<number>(0.9);
  const [bracketDropTarget, setBracketDropTarget] = useState<number | null>(null);
  const [isDraggingReplays, setIsDraggingReplays] = useState<boolean>(false);
  const [recentDropSetId, setRecentDropSetId] = useState<number | null>(null);
  const [isBracketPanning, setIsBracketPanning] = useState<boolean>(false);
  const [replaySetIds, setReplaySetIds] = useState<number[]>([]);
  const [replayStreamUpdate, setReplayStreamUpdate] = useState<ReplayStreamUpdate | null>(null);
  const [replayStreamStartedAt, setReplayStreamStartedAt] = useState<number | null>(null);
  const [broadcastSelections, setBroadcastSelections] = useState<Record<number, boolean>>({});
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);

  // ── Refs ──────────────────────────────────────────────────────────────

  const bracketDragDepth = useRef(0);
  const dropFlashTimer = useRef<number | null>(null);
  const dropScaleFactor = useRef(1);
  const bracketScrollRef = useRef<HTMLDivElement | null>(null);
  const bracketPanTarget = useRef<HTMLElement | null>(null);
  const bracketPanStartX = useRef(0);
  const bracketPanScrollLeft = useRef(0);
  const appliedReplayPaths = useRef<Set<string>>(new Set());

  // ── Memos ─────────────────────────────────────────────────────────────

  const bracketSetDetails = useMemo(
    () => bracketState?.sets.find((set) => set.id === bracketSetDetailsId) ?? null,
    [bracketState, bracketSetDetailsId],
  );

  const bracketSetsById = useMemo(() => {
    if (!bracketState) {
      return new Map<number, StartggSimState["sets"][number]>();
    }
    return new Map(bracketState.sets.map((set) => [set.id, set]));
  }, [bracketState]);

  const bracketSetIsPending = bracketSetDetails?.state === "pending";
  const bracketSetIsCompleted =
    bracketSetDetails?.state === "completed" || bracketSetDetails?.state === "skipped";

  const bracketSetDetailsJson = useMemo(
    () => (bracketSetDetails ? JSON.stringify(bracketSetDetails, null, 2) : ""),
    [bracketSetDetails],
  );

  const replaySet = useMemo(() => new Set(replaySetIds), [replaySetIds]);

  const broadcastEntrants = useMemo(() => {
    if (!bracketState) {
      return [];
    }
    return [...bracketState.entrants].sort((a, b) => a.seed - b.seed);
  }, [bracketState]);

  const broadcastActiveCount = useMemo(
    () => broadcastEntrants.filter((entrant) => broadcastSelections[entrant.id]).length,
    [broadcastEntrants, broadcastSelections],
  );

  const bracketRounds = useMemo(() => {
    if (!bracketState) {
      return null;
    }
    const winners = new Map<number, StartggSimState["sets"]>();
    const losers = new Map<number, StartggSimState["sets"]>();
    const finals: StartggSimState["sets"] = [];

    for (const set of bracketState.sets) {
      if (set.round === 0) {
        finals.push(set);
        continue;
      }
      if (set.round > 0) {
        const list = winners.get(set.round) ?? [];
        list.push(set);
        winners.set(set.round, list);
      } else {
        const list = losers.get(set.round) ?? [];
        list.push(set);
        losers.set(set.round, list);
      }
    }

    const winnersRounds = Array.from(winners.entries()).sort((a, b) => a[0] - b[0]);
    const losersRounds = Array.from(losers.entries()).sort(
      (a, b) => Math.abs(a[0]) - Math.abs(b[0]),
    );
    if (finals.length > 0) {
      finals.sort((a, b) => {
        const aReset = a.roundLabel.toLowerCase().includes("reset");
        const bReset = b.roundLabel.toLowerCase().includes("reset");
        if (aReset === bReset) {
          return a.id - b.id;
        }
        return aReset ? 1 : -1;
      });
      winnersRounds.push([0, finals]);
    }

    const winnersBase = winnersRounds[0]?.[1]?.length ?? 0;
    const losersBase = losersRounds[0]?.[1]?.length ?? 0;

    return { winnersRounds, losersRounds, winnersBase, losersBase };
  }, [bracketState]);

  // ── Functions ─────────────────────────────────────────────────────────

  function applyNormalizedState(next: StartggSimState) {
    setBracketState(next);
  }

  async function refreshBracketState() {
    setBracketStatus("");
    setIsRefreshing(true);
    try {
      console.time("refreshBracketState:invoke");
      const res = await invoke("startgg_sim_raw_state");
      console.timeEnd("refreshBracketState:invoke");
      console.time("refreshBracketState:normalize");
      const normalized = normalizeStartggResponse(res);
      console.timeEnd("refreshBracketState:normalize");
      if (normalized) {
        console.time("refreshBracketState:setState");
        applyNormalizedState(normalized);
        console.timeEnd("refreshBracketState:setState");
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Bracket update failed: ${msg}`);
    } finally {
      setIsRefreshing(false);
    }
  }

  async function resetBracketState(pathOverride?: string, autoCompleteOverride?: boolean) {
    setBracketStatus("Resetting bracket…");
    try {
      const payload = pathOverride ? { configPath: pathOverride } : {};
      const res = await invoke("startgg_sim_raw_reset", payload);
      const normalized = normalizeStartggResponse(res);
      if (normalized) {
        applyNormalizedState(normalized);
      }
      loadReplaySets(pathOverride ?? selectedBracketPath ?? DEFAULT_TEST_BRACKET_PATH);
      setBracketStatus("Bracket reset.");
      const shouldAutoComplete = autoCompleteOverride ?? config.autoCompleteBracket;
      if (shouldAutoComplete) {
        await completeBracket();
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Bracket reset failed: ${msg}`);
    }
  }

  async function completeBracket() {
    setBracketStatus("");
    try {
      const res = await invoke("startgg_sim_raw_complete_bracket");
      const normalized = normalizeStartggResponse(res);
      if (normalized) {
        applyNormalizedState(normalized);
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Auto-complete failed: ${msg}`);
    }
  }

  async function streamBracketReplay(setId: number) {
    setBracketStatus("Starting replay stream…");
    setReplayStreamUpdate({ type: "start", setId });
    setReplayStreamStartedAt(Date.now());
    const configPath =
      selectedBracketPath.trim() ||
      config.testBracketPath.trim() ||
      DEFAULT_TEST_BRACKET_PATH;
    try {
      const res = await invoke<SpoofReplayResult>("spoof_bracket_set_replays", {
        configPath,
        setId,
      });
      const missing =
        res.missing > 0 ? ` (${res.missing} missing replay${res.missing === 1 ? "" : "s"})` : "";
      setBracketStatus(`Streaming ${res.started} replay${res.started === 1 ? "" : "s"}${missing}.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Replay start failed: ${msg}`);
    }
  }

  async function streamBracketReplayGame(
    setId: number,
    replayPath: string,
    replayIndex: number,
    replayTotal: number,
  ) {
    setBracketSetActionStatus("");
    appliedReplayPaths.current.delete(replayPath);
    setReplayStreamUpdate({ type: "start", setId, replayPath, replayIndex, replayTotal });
    setReplayStreamStartedAt(Date.now());
    try {
      await invoke<SpoofReplayResult>("spoof_bracket_set_replay", {
        setId,
        replayPath,
        replayIndex,
        replayTotal,
      });
      setBracketSetActionStatus(`Streaming game ${replayIndex}.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketSetActionStatus(`Replay start failed: ${msg}`);
    }
  }

  async function applyStartggUpdate(
    action: () => Promise<unknown>,
    successMessage?: string,
  ) {
    setBracketSetActionStatus("");
    try {
      const res = await action();
      const normalized = normalizeStartggResponse(res);
      if (normalized) {
        applyNormalizedState(normalized);
      }
      if (successMessage) {
        setBracketSetActionStatus(successMessage);
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketSetActionStatus(`Update failed: ${msg}`);
    }
  }

  async function startMatchForSet(setId: number) {
    await applyStartggUpdate(
      () => invoke("startgg_sim_raw_start_set", { setId }),
      "Match started.",
    );
  }

  function isReplayInProgress(setId: number) {
    if (!replayStreamUpdate || replayStreamUpdate.setId !== setId) {
      return false;
    }
    return replayStreamUpdate.type === "start" || replayStreamUpdate.type === "progress";
  }

  async function stepBracketSet(setId: number) {
    if (!bracketSetDetails || bracketSetDetails.id !== setId) {
      setBracketSetActionStatus("Select a set first.");
      return;
    }
    if (bracketSetIsCompleted) {
      setBracketSetActionStatus("Set already completed.");
      return;
    }
    const scoreA = bracketSetDetails.slots[0]?.score ?? 0;
    const scoreB = bracketSetDetails.slots[1]?.score ?? 0;
    const nextIndex = scoreA + scoreB;
    const hasReplays = bracketSetReplayPaths.length > 0;
    const nextReplay = hasReplays ? bracketSetReplayPaths[nextIndex] : null;
    if (hasReplays && isReplayInProgress(setId)) {
      const replayPath = replayStreamUpdate?.replayPath;
      if (replayPath) {
        appliedReplayPaths.current.add(replayPath);
        await applyReplayResult(setId, replayPath, { successMessage: "Score updated." });
        return;
      }
      setBracketSetActionStatus("Replay already streaming.");
      return;
    }
    if (hasReplays && nextReplay) {
      if (bracketSetIsPending) {
        await startMatchForSet(setId);
      }
      await streamBracketReplayGame(
        setId,
        nextReplay,
        nextIndex + 1,
        bracketSetReplayPaths.length,
      );
      return;
    }
    await applyStartggUpdate(
      () => invoke("startgg_sim_raw_step_set", { setId }),
      "Set advanced.",
    );
  }

  async function finalizeSetFromReference(setId: number) {
    await applyStartggUpdate(
      () => invoke("startgg_sim_raw_finalize_reference_set", { setId }),
      "Set finalized.",
    );
  }

  async function resetSet(setId: number) {
    await applyStartggUpdate(
      () =>
        invoke("startgg_sim_raw_reset_set", {
          setId,
        }),
      "Set reset.",
    );
  }

  async function applyReplayResult(
    setId: number,
    replayPath?: string | null,
    options?: { successMessage?: string },
  ) {
    if (!replayPath) {
      return;
    }
    await applyStartggUpdate(
      () =>
        invoke("startgg_sim_raw_apply_replay_result", {
          setId,
          replayPath,
        }),
      options?.successMessage,
    );
  }

  async function openBracketSetDetails(setId: number) {
    setBracketSetDetailsId(setId);
    setBracketSetReplayPaths([]);
    setBracketSetDetailsStatus("Loading replay paths…");
    const configPath =
      selectedBracketPath.trim() ||
      config.testBracketPath.trim() ||
      DEFAULT_TEST_BRACKET_PATH;
    try {
      const paths = await invoke<string[]>("list_bracket_set_replay_paths", {
        configPath,
        setId,
      });
      setBracketSetReplayPaths(paths);
      setBracketSetDetailsStatus(paths.length === 0 ? "No replay paths attached." : "");
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketSetDetailsStatus(`Replay lookup failed: ${msg}`);
    }
  }

  function closeBracketSetDetails() {
    setBracketSetDetailsId(null);
    setBracketSetReplayPaths([]);
    setBracketSetDetailsStatus("");
  }

  function openBracketSettings() {
    setBracketSettingsOpen(true);
  }

  function closeBracketSettings() {
    setBracketSettingsOpen(false);
  }

  function toggleBroadcast(entrantId: number) {
    setBroadcastSelections((prev) => ({
      ...prev,
      [entrantId]: !prev[entrantId],
    }));
  }

  async function loadReplaySets(configPath: string) {
    try {
      const res = await invoke<number[]>("list_bracket_replay_sets", {
        configPath,
      });
      setReplaySetIds(res);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Replay map load failed: ${msg}`);
    }
  }

  async function saveReplayPathsToSet(
    setId: number,
    replayPaths: string[],
    options: { ignored?: number; missingPath?: boolean } = {},
  ) {
    if (replayPaths.length === 0) {
      if (options.missingPath) {
        setBracketStatus("Drop .slp files from your file manager so I can read the full path.");
      } else {
        setBracketStatus("No .slp files found in that drop.");
      }
      return;
    }

    const configPath =
      selectedBracketPath.trim() ||
      config.testBracketPath.trim() ||
      DEFAULT_TEST_BRACKET_PATH;
    setBracketStatus(`Saving ${replayPaths.length} replay${replayPaths.length === 1 ? "" : "s"}…`);
    try {
      await invoke("update_bracket_set_replays", {
        configPath,
        setId,
        replayPaths,
      });
      setReplaySetIds((prev) => {
        if (prev.includes(setId)) {
          return prev;
        }
        return [...prev, setId];
      });
      flashDropSuccess(setId);
      await loadReplaySets(configPath);
      const ignored = options.ignored ?? 0;
      const ignoredLabel = ignored > 0 ? ` (${ignored} ignored)` : "";
      setBracketStatus(
        `Saved ${replayPaths.length} replay${replayPaths.length === 1 ? "" : "s"} to set ${setId}.${ignoredLabel}`,
      );
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Save replays failed: ${msg}`);
    }
  }

  function flashDropSuccess(setId: number) {
    setRecentDropSetId(setId);
    if (dropFlashTimer.current) {
      window.clearTimeout(dropFlashTimer.current);
    }
    dropFlashTimer.current = window.setTimeout(() => {
      setRecentDropSetId(null);
    }, 1400);
  }

  function hasFileDrag(event: DragEvent<HTMLElement>) {
    const items = Array.from(event.dataTransfer.items ?? []);
    if (items.some((item) => item.kind === "file")) {
      return true;
    }
    const types = Array.from(event.dataTransfer.types ?? []);
    if (types.includes("Files")) {
      return true;
    }
    if (types.some((type) => type.includes("text/uri-list"))) {
      return true;
    }
    return event.dataTransfer.files.length > 0;
  }

  function resetBracketDragState() {
    bracketDragDepth.current = 0;
    setIsDraggingReplays(false);
    setBracketDropTarget(null);
  }

  function handleBracketDragEnter(event: DragEvent<HTMLDivElement>) {
    if (!hasFileDrag(event)) {
      return;
    }
    bracketDragDepth.current += 1;
    setIsDraggingReplays(true);
  }

  function handleBracketDragLeave(event: DragEvent<HTMLDivElement>) {
    if (bracketDragDepth.current === 0) {
      return;
    }
    bracketDragDepth.current = Math.max(0, bracketDragDepth.current - 1);
    if (bracketDragDepth.current === 0) {
      resetBracketDragState();
    }
  }

  function handleSetDragOver(event: DragEvent<HTMLDivElement>, setId: number) {
    if (!hasFileDrag(event)) {
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setBracketDropTarget(setId);
    setIsDraggingReplays(true);
  }

  function handleSetDragLeave(setId: number) {
    if (bracketDropTarget === setId) {
      setBracketDropTarget(null);
    }
  }

  async function handleSetDrop(event: DragEvent<HTMLDivElement>, setId: number) {
    event.preventDefault();
    event.stopPropagation();
    resetBracketDragState();

    const files = Array.from(event.dataTransfer.files) as Array<File & { path?: string }>;
    const replayPaths: string[] = [];
    let missingPath = false;
    let ignored = 0;

    for (const file of files) {
      const rawPath = file.path ?? "";
      const name = rawPath || file.name;
      if (!name) {
        continue;
      }
      if (!isReplayFilename(name)) {
        ignored += 1;
        continue;
      }
      if (!rawPath) {
        missingPath = true;
        continue;
      }
      replayPaths.push(rawPath);
    }

    if (replayPaths.length === 0) {
      const uriList = event.dataTransfer.getData("text/uri-list");
      if (uriList) {
        const lines = uriList
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter((line) => line && !line.startsWith("#"));
        for (const uri of lines) {
          try {
            const url = new URL(uri);
            if (url.protocol !== "file:") {
              continue;
            }
            let path = decodeURIComponent(url.pathname);
            if (path.startsWith("/") && path.length > 2 && path[2] === ":") {
              path = path.slice(1);
            }
            if (!isReplayFilename(path)) {
              ignored += 1;
              continue;
            }
            replayPaths.push(path);
          } catch {
            ignored += 1;
          }
        }
      }
    }

    await saveReplayPathsToSet(setId, replayPaths, { ignored, missingPath });
  }

  function resolveBracketScrollTarget(target: EventTarget | null) {
    const root = bracketScrollRef.current;
    if (!root) {
      return null;
    }
    let node: HTMLElement | null = null;
    if (target instanceof HTMLElement) {
      node = target;
    } else if (target && (target as Node).parentElement) {
      node = (target as Node).parentElement;
    }
    while (node && root.contains(node)) {
      if (node.scrollWidth > node.clientWidth + 1) {
        return node;
      }
      node = node.parentElement;
    }
    if (root.scrollWidth > root.clientWidth + 1) {
      return root;
    }
    return root;
  }

  function startBracketPan(event: {
    button: number;
    clientX: number;
    target: EventTarget | null;
    preventDefault?: () => void;
    stopPropagation?: () => void;
  }) {
    if (event.button !== 1) {
      return;
    }
    if (bracketPanTarget.current) {
      return;
    }
    const scrollTarget = resolveBracketScrollTarget(event.target);
    if (!scrollTarget) {
      return;
    }
    event.preventDefault?.();
    event.stopPropagation?.();
    bracketPanTarget.current = scrollTarget;
    bracketPanStartX.current = event.clientX;
    bracketPanScrollLeft.current = scrollTarget.scrollLeft;
    setIsBracketPanning(true);
  }

  function handleBracketPanStart(event: MouseEvent<HTMLDivElement>) {
    startBracketPan(event);
  }

  function handleBracketPanStop() {
    if (!isBracketPanning) {
      return;
    }
    bracketPanTarget.current = null;
    setIsBracketPanning(false);
  }

  function getSetIdFromPoint(x: number, y: number) {
    const el = document.elementFromPoint(x, y);
    const setEl = el?.closest?.("[data-set-id]") as HTMLElement | null;
    if (!setEl) {
      return null;
    }
    const raw = setEl.dataset.setId ?? "";
    const id = Number(raw);
    return Number.isFinite(id) ? id : null;
  }

  function updateDropTargetFromPosition(position: { x: number; y: number }) {
    const scale = dropScaleFactor.current || 1;
    const x = position.x / scale;
    const y = position.y / scale;
    const next = getSetIdFromPoint(x, y);
    setBracketDropTarget((prev) => (prev === next ? prev : next));
  }

  function columnHeightFn(baseCount: number): string | undefined {
    if (!baseCount) {
      return undefined;
    }
    return `${baseCount * BRACKET_SET_HEIGHT}px`;
  }

  async function openEventLink(url: string) {
    if (!url) {
      return;
    }
    try {
      await openUrl(url);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Open event link failed: ${msg}`);
    }
  }

  function resolveSlotLabelLocal(
    slot: StartggSimState["sets"][number]["slots"][number],
    visitedSourceIds: Set<number> = new Set(),
  ): string {
    return resolveSlotLabel(slot, bracketSetsById, visitedSourceIds);
  }

  async function cancelReplayStream() {
    const setId = replayStreamUpdate?.setId;
    if (!setId) {
      setBracketStatus("No active replay stream to cancel.");
      return;
    }
    setBracketStatus("Stopping replay stream…");
    try {
      const cancelled = await invoke<number>("cancel_spoof_bracket_set_replays", { setId });
      if (cancelled > 0) {
        setBracketStatus("Replay stream cancelled.");
      } else {
        setBracketStatus("No active replay stream to cancel.");
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Cancel failed: ${msg}`);
    }
  }

  // ── Effects ───────────────────────────────────────────────────────────

  // Bracket view initialization: load config and reset/refresh bracket
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    (async () => {
      let loaded: AppConfig | null = null;
      try {
        const res = await invoke<AppConfig>("load_config");
        loaded = {
          ...res,
          startggLink: res.startggLink ?? "",
          startggToken: res.startggToken ?? "",
          startggPolling: res.startggPolling ?? false,
          autoStream: res.autoStream ?? true,
          testMode: res.testMode ?? false,
          autoCompleteBracket: res.autoCompleteBracket ?? true,
          testBracketPath: (res.testBracketPath ?? "").trim() || DEFAULT_TEST_BRACKET_PATH,
        };
      } catch {
        // config load failed, proceed with defaults
      }
      const bracketPath =
        (loaded?.testBracketPath ?? selectedBracketPath ?? DEFAULT_TEST_BRACKET_PATH).trim() ||
        DEFAULT_TEST_BRACKET_PATH;
      if (loaded?.autoCompleteBracket) {
        await resetBracketState(bracketPath, true);
      } else {
        await refreshBracketState();
      }
    })();
  }, [isBracketView]);

  // Load replay sets when bracket view or path changes
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    loadReplaySets(selectedBracketPath || DEFAULT_TEST_BRACKET_PATH);
  }, [isBracketView, selectedBracketPath]);

  // Sync broadcast selections with bracket entrants
  useEffect(() => {
    if (!isBracketView || !bracketState) {
      return;
    }
    setBroadcastSelections((prev) => {
      const next: Record<number, boolean> = {};
      for (const entrant of bracketState.entrants) {
        next[entrant.id] = prev[entrant.id] ?? false;
      }
      return next;
    });
  }, [isBracketView, bracketState]);

  // Push broadcast player selections to backend
  useEffect(() => {
    if (!isBracketView || !bracketState) {
      return;
    }
    const players: BroadcastPlayerSelection[] = bracketState.entrants
      .filter((entrant) => broadcastSelections[entrant.id])
      .map((entrant) => ({
        id: entrant.id,
        name: entrant.name,
        slippiCode: entrant.slippiCode,
      }));
    invoke("set_broadcast_players", { players }).catch((e) => {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Broadcast update failed: ${msg}`);
    });
  }, [isBracketView, bracketState, broadcastSelections]);

  // Tauri drag/drop event listener for bracket view
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    (async () => {
      try {
        const win = getCurrentWindow();
        dropScaleFactor.current = await win.scaleFactor();
        const webview = getCurrentWebview();
        unlisten = await webview.onDragDropEvent(async (event) => {
          if (cancelled) {
            return;
          }
          const payload = event.payload;
          if (payload.type === "enter" || payload.type === "over") {
            setIsDraggingReplays(true);
            updateDropTargetFromPosition(payload.position);
            return;
          }
          if (payload.type === "leave") {
            resetBracketDragState();
            return;
          }
          if (payload.type === "drop") {
            resetBracketDragState();
            const setId = (() => {
              const scale = dropScaleFactor.current || 1;
              const x = payload.position.x / scale;
              const y = payload.position.y / scale;
              return getSetIdFromPoint(x, y);
            })();
            if (!setId) {
              setBracketStatus("Drop files directly onto a bracket set.");
              return;
            }
            const replayPathsFromDrop = payload.paths.filter((path) => isReplayFilename(path));
            const ignoredCount = payload.paths.length - replayPathsFromDrop.length;
            await saveReplayPathsToSet(setId, replayPathsFromDrop, { ignored: ignoredCount });
          }
        });
      } catch (e) {
        const msg =
          e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
        setBracketStatus(`Drag/drop hook failed: ${msg}`);
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [isBracketView, selectedBracketPath, config.testBracketPath]);

  // Bracket panning: mousemove / mouseup / blur listeners
  useEffect(() => {
    if (!isBracketView || !isBracketPanning) {
      return;
    }
    const handleMove = (event: globalThis.MouseEvent) => {
      const target = bracketPanTarget.current;
      if (!target) {
        return;
      }
      const dx = event.clientX - bracketPanStartX.current;
      target.scrollLeft = bracketPanScrollLeft.current - dx;
    };
    const handleUp = (event: globalThis.MouseEvent) => {
      if (event.button === 1 || event.buttons === 0) {
        handleBracketPanStop();
      }
    };
    const handleBlur = () => {
      handleBracketPanStop();
    };
    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
    window.addEventListener("blur", handleBlur);
    return () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, [isBracketView, isBracketPanning]);

  // Middle-click panning initiation
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    const handleDown = (event: globalThis.MouseEvent) => {
      if (event.button !== 1) {
        return;
      }
      const root = bracketScrollRef.current;
      if (!root) {
        return;
      }
      if (!root.contains(event.target as Node)) {
        return;
      }
      startBracketPan(event);
    };
    window.addEventListener("mousedown", handleDown, true);
    return () => {
      window.removeEventListener("mousedown", handleDown, true);
    };
  }, [isBracketView]);

  // Bracket set details: close if set disappears
  useEffect(() => {
    if (bracketSetDetailsId === null) {
      return;
    }
    if (!bracketSetDetails) {
      setBracketSetDetailsId(null);
    }
  }, [bracketSetDetailsId, bracketSetDetails]);

  // Clear bracket set action status when set data changes
  useEffect(() => {
    if (!bracketSetDetails) {
      return;
    }
    setBracketSetActionStatus("");
  }, [bracketSetDetails?.id, bracketSetDetails?.updatedAtMs]);

  // Bracket set details: Escape key handler
  useEffect(() => {
    if (bracketSetDetailsId === null) {
      return;
    }
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setBracketSetDetailsId(null);
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => {
      window.removeEventListener("keydown", handleKey);
    };
  }, [bracketSetDetailsId]);

  // Bracket view replay stream listener
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    let unlisten: UnlistenFn | null = null;
    listen<ReplayStreamUpdate>("spoof-replay-progress", (event) => {
      const payload = event.payload;
      setReplayStreamUpdate(payload);
      if (payload?.type === "start") {
        setReplayStreamStartedAt(Date.now());
      }
      if (
        payload?.type === "complete" &&
        payload.setId &&
        payload.replayPath &&
        config.testMode
      ) {
        if (appliedReplayPaths.current.has(payload.replayPath)) {
          appliedReplayPaths.current.delete(payload.replayPath);
          return;
        }
        applyReplayResult(payload.setId, payload.replayPath);
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        unlisten = null;
      });
    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [isBracketView]);

  // Bracket keyboard zoom (Ctrl+/Ctrl-/Ctrl0)
  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    const clampZoom = (value: number) =>
      Math.min(BRACKET_ZOOM_MAX, Math.max(BRACKET_ZOOM_MIN, value));
    const handleZoomKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) {
        return;
      }
      if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        setBracketZoom((prev) => clampZoom(prev + BRACKET_ZOOM_STEP));
        return;
      }
      if (event.key === "-") {
        event.preventDefault();
        setBracketZoom((prev) => clampZoom(prev - BRACKET_ZOOM_STEP));
        return;
      }
      if (event.key === "0") {
        event.preventDefault();
        setBracketZoom(1);
      }
    };
    window.addEventListener("keydown", handleZoomKey);
    return () => window.removeEventListener("keydown", handleZoomKey);
  }, [isBracketView]);

  // ── Return ────────────────────────────────────────────────────────────

  return {
    bracketState,
    bracketStatus,
    bracketSettingsOpen,
    bracketSetDetailsId,
    bracketSetReplayPaths,
    bracketSetDetailsStatus,
    bracketSetActionStatus,
    bracketZoom,
    setBracketZoom,
    bracketDropTarget,
    isDraggingReplays,
    recentDropSetId,
    isBracketPanning,
    replaySetIds,
    replayStreamUpdate,
    replayStreamStartedAt,
    broadcastSelections,
    bracketScrollRef,
    bracketSetDetails,
    bracketSetsById,
    bracketSetDetailsJson,
    bracketSetIsPending,
    bracketSetIsCompleted,
    bracketRounds,
    broadcastEntrants,
    broadcastActiveCount,
    replaySet,
    isRefreshing,
    setBracketStatus,
    applyNormalizedState,
    refreshBracketState,
    resetBracketState,
    completeBracket,
    streamBracketReplay,
    streamBracketReplayGame,
    applyStartggUpdate,
    startMatchForSet,
    stepBracketSet,
    finalizeSetFromReference,
    resetSet,
    applyReplayResult,
    openBracketSetDetails,
    closeBracketSetDetails,
    openBracketSettings,
    closeBracketSettings,
    toggleBroadcast,
    loadReplaySets,
    saveReplayPathsToSet,
    flashDropSuccess,
    hasFileDrag,
    resetBracketDragState,
    handleBracketDragEnter,
    handleBracketDragLeave,
    handleSetDragOver,
    handleSetDragLeave,
    handleSetDrop,
    handleBracketPanStart,
    handleBracketPanStop,
    startBracketPan,
    resolveBracketScrollTarget,
    getSetIdFromPoint,
    updateDropTargetFromPosition,
    columnHeight: columnHeightFn,
    isReplayInProgress,
    openEventLink,
    resolveSlotLabel: resolveSlotLabelLocal,
    formatSetState,
    columnLabel,
    cancelReplayStream,
  };
}
