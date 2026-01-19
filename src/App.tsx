import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type {
  AppConfig,
  BracketConfigInfo,
  ReplayStreamUpdate,
  SpoofReplayResult,
  Setup,
  SlippiStream,
  SlippiWindowInfo,
  StartggSimState,
} from "./types/overlay";
import { normalizeStartggResponse } from "./startggAdapter";

const DEFAULT_TEST_BRACKET_PATH = "test_brackets/test_bracket_2.json";
const BRACKET_ZOOM_MIN = 0.5;
const BRACKET_ZOOM_MAX = 1.5;
const BRACKET_ZOOM_STEP = 0.05;
const BRACKET_SET_HEIGHT = 88;

export default function App() {
  const isBracketView = useMemo(() => {
    const params = new URLSearchParams(window.location.search);
    return params.get("view") === "bracket";
  }, []);
  const [setups, setSetups] = useState<Setup[]>([]);
  const [setupStatus, setSetupStatus] = useState<string>("No setups yet. Add one to start.");
  const [streams, setStreams] = useState<SlippiStream[]>([]);
  const [streamsStatus, setStreamsStatus] = useState<string>("Scan for live Slippi streams.");
  const [topStatus, setTopStatus] = useState<string>("");
  const [windowInfo, setWindowInfo] = useState<SlippiWindowInfo | null>(null);
  const [windowStatus, setWindowStatus] = useState<string>("");
  const [launchStatus, setLaunchStatus] = useState<string>("");
  const [dolphinLaunchStatus, setDolphinLaunchStatus] = useState<string>("");
  const [slippiRefreshStatus, setSlippiRefreshStatus] = useState<string>("");
  const [watchStatus, setWatchStatus] = useState<string>("");
  const [streamSetupSelections, setStreamSetupSelections] = useState<Record<string, number>>({});
  const [settingsOpen, setSettingsOpen] = useState<boolean>(false);
  const [bracketSettingsOpen, setBracketSettingsOpen] = useState<boolean>(false);
  const [configStatus, setConfigStatus] = useState<string>("");
  const [bracketStatus, setBracketStatus] = useState<string>("");
  const [bracketState, setBracketState] = useState<StartggSimState | null>(null);
  const [bracketConfigs, setBracketConfigs] = useState<BracketConfigInfo[]>([]);
  const [selectedBracketPath, setSelectedBracketPath] = useState<string>(
    DEFAULT_TEST_BRACKET_PATH,
  );
  const [bracketZoom, setBracketZoom] = useState<number>(0.9);
  const [replaySetIds, setReplaySetIds] = useState<number[]>([]);
  const [replayStreamUpdate, setReplayStreamUpdate] = useState<ReplayStreamUpdate | null>(null);
  const [replayStreamStartedAt, setReplayStreamStartedAt] = useState<number | null>(null);
  const [config, setConfig] = useState<AppConfig>({
    dolphinPath: "",
    ssbmIsoPath: "",
    slippiLauncherPath: "",
    spectateFolderPath: "",
    testMode: false,
    testBracketPath: DEFAULT_TEST_BRACKET_PATH,
    autoCompleteBracket: true,
  });
  const replaySet = useMemo(() => new Set(replaySetIds), [replaySetIds]);

  function updateConfig<K extends keyof AppConfig>(key: K, value: AppConfig[K]) {
    setConfig((prev) => ({ ...prev, [key]: value }));
  }

  async function browsePath(
    key: keyof AppConfig,
    options: { directory: boolean; title: string },
  ) {
    try {
      const current = config[key].trim();
      const selected = await openDialog({
        directory: options.directory,
        multiple: false,
        title: options.title,
        defaultPath: current || undefined,
      });
      if (typeof selected === "string") {
        updateConfig(key, selected);
      } else if (Array.isArray(selected) && selected.length > 0) {
        updateConfig(key, selected[0]);
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setConfigStatus(`Open dialog failed: ${msg}`);
    }
  }

  async function refreshStreams() {
    setWindowStatus("");
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
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setWindowInfo(null);
      setWindowStatus(`Window search failed: ${msg}`);
      setStreams([]);
      setStreamsStatus("Could not find Slippi Launcher. Launch it, then refresh.");
      return;
    }

    setStreamsStatus("Scanning for Slippi streams…");
    try {
      const res = await invoke<SlippiStream[]>("scan_slippi_streams");
      setStreams(res);
      setStreamsStatus(res.length === 0 ? "No streams detected from Slippi Launcher." : `Found ${res.length} stream(s).`);
    } catch (e) {
      setStreams([]);
      setStreamsStatus("Could not scan streams. Make sure Slippi is running, then refresh.");
    }
  }

  async function loadSetups() {
    try {
      const res = await invoke<Setup[]>("list_setups");
      setSetups(res);
      setSetupStatus(res.length === 0 ? "No setups yet. Add one to start." : "");
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setSetupStatus(`Load setups failed: ${msg}`);
    }
  }

  async function addSetup() {
    setSetupStatus("Creating setup…");
    try {
      const setup = await invoke<Setup>("create_setup");
      setSetups((prev) => [...prev, setup]);
      setSetupStatus(`Setup ${setup.id} created.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setSetupStatus(`Add setup failed: ${msg}`);
    }
  }

  async function removeSetup(id: number) {
    setSetupStatus(`Deleting setup ${id}…`);
    try {
      await invoke("delete_setup", { id });
      setSetups((prev) => prev.filter((s) => s.id !== id));
      setSetupStatus(`Setup ${id} deleted.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setSetupStatus(`Delete setup failed: ${msg}`);
    }
  }

  async function clearSetup(id: number) {
    setSetupStatus(`Clearing setup ${id}…`);
    try {
      const updated = await invoke<Setup>("clear_setup_assignment", { setupId: id });
      setSetups((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      setSetupStatus(`Setup ${id} cleared.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setSetupStatus(`Clear setup failed: ${msg}`);
    }
  }

  async function launchSlippi() {
    setLaunchStatus("Launching Slippi Launcher…");
    setWindowStatus("");
    try {
      await invoke("launch_slippi_app");
      setLaunchStatus("Slippi launch requested. Give it a moment, then hit Refresh.");
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setLaunchStatus(`Launch failed: ${msg}`);
    }
  }

  async function launchDolphin() {
    setDolphinLaunchStatus("Launching Dolphin…");
    try {
      await invoke("launch_dolphin_cli");
      setDolphinLaunchStatus("Dolphin launch requested.");
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setDolphinLaunchStatus(`Dolphin launch failed: ${msg}`);
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
    } catch (e) {
      setSlippiRefreshStatus("Slippi refresh failed. Make sure Slippi is running.");
    }
    await refreshStreams();
    if (clicked) {
      setSlippiRefreshStatus("");
    }
  }

  async function loadConfig(): Promise<AppConfig | null> {
    try {
      const res = await invoke<AppConfig>("load_config");
      const bracketPath =
        (res.testBracketPath ?? "").trim() || DEFAULT_TEST_BRACKET_PATH;
      const nextConfig = { ...res, testBracketPath: bracketPath };
      setConfig(nextConfig);
      setSelectedBracketPath(bracketPath);
      setConfigStatus("");
      return nextConfig;
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setConfigStatus(`Load settings failed: ${msg}`);
      return null;
    }
  }

  async function saveConfig(nextConfig: AppConfig = config) {
    setConfigStatus("Saving settings…");
    try {
      const res = await invoke<AppConfig>("save_config", { config: nextConfig });
      setConfig(res);
      setConfigStatus("Settings saved.");
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setConfigStatus(`Save settings failed: ${msg}`);
    }
  }

  async function toggleTestMode() {
    const nextConfig = { ...config, testMode: !config.testMode };
    setConfig(nextConfig);
    await saveConfig(nextConfig);
  }

  async function setAutoCompleteBracket(enabled: boolean) {
    const nextConfig = { ...config, autoCompleteBracket: enabled };
    setConfig(nextConfig);
    await saveConfig(nextConfig);
  }

  function openSettings() {
    setSettingsOpen(true);
    setConfigStatus("");
    loadConfig();
  }

  function closeSettings() {
    setSettingsOpen(false);
    setConfigStatus("");
  }

  function openBracketSettings() {
    setBracketSettingsOpen(true);
  }

  function closeBracketSettings() {
    setBracketSettingsOpen(false);
  }

  function getStreamSetupId(streamId: string) {
    if (streamSetupSelections[streamId] !== undefined) {
      return streamSetupSelections[streamId];
    }
    return setups[0]?.id ?? null;
  }

  async function watchStream(stream: SlippiStream) {
    const setupId = getStreamSetupId(stream.id);
    if (setupId === null) {
      setWatchStatus("Select a setup first.");
      return;
    }
    const label = stream.p1Tag || stream.p1Code || stream.windowTitle || stream.id;
    setWatchStatus(`Assigning "${label}" to setup ${setupId} and launching dolphin-${setupId}…`);
    try {
      const updated = await invoke<Setup>("assign_stream_to_setup", {
        setupId,
        stream,
      });
      setSetups((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      setWatchStatus(`Assigned "${label}" to setup ${setupId}. Dolphin launch requested.`);
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setWatchStatus(`Assign failed: ${msg}`);
    }
  }

  async function handleSetupSelect(streamId: string, value: string) {
    setStreamSetupSelections((prev) => {
      const next = { ...prev };
      if (!value) {
        delete next[streamId];
      } else {
        next[streamId] = Number(value);
      }
      return next;
    });
    if (value) {
      setWatchStatus(`Setup ${value} selected.`);
    } else {
      setWatchStatus("");
    }
  }

  useEffect(() => {
    if (setups.length === 0) {
      setStreamSetupSelections({});
      return;
    }
    setStreamSetupSelections((prev) => {
      const valid = new Set(setups.map((s) => s.id));
      const next: Record<string, number> = {};
      for (const [streamId, setupId] of Object.entries(prev)) {
        if (valid.has(setupId)) {
          next[streamId] = setupId;
        }
      }
      return next;
    });
  }, [setups]);

  useEffect(() => {
    if (isBracketView) {
      return;
    }
    loadSetups();
    refreshStreams();
    loadConfig();
  }, [isBracketView]);

  useEffect(() => {
    if (isBracketView || !settingsOpen || !config.testMode) {
      return;
    }
    loadBracketConfigs();
  }, [isBracketView, settingsOpen, config.testMode]);

  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    (async () => {
      const loaded = await loadConfig();
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

  useEffect(() => {
    if (!isBracketView) {
      return;
    }
    loadReplaySets(selectedBracketPath || DEFAULT_TEST_BRACKET_PATH);
  }, [isBracketView, selectedBracketPath]);

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

  async function openBracketWindow() {
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
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setTopStatus(`Open bracket window failed: ${msg}`);
    }
  }

  function applyNormalizedState(next: StartggSimState) {
    setBracketState(next);
  }

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

  async function loadBracketConfigs() {
    try {
      const res = await invoke<BracketConfigInfo[]>("list_bracket_configs");
      setBracketConfigs(res);
      if (selectedBracketPath && !res.some((config) => config.path === selectedBracketPath)) {
        setSelectedBracketPath("");
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      if (isBracketView) {
        setBracketStatus(`Load bracket configs failed: ${msg}`);
      } else {
        setConfigStatus(`Load bracket configs failed: ${msg}`);
      }
    }
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

  async function handleBracketSelect(path: string) {
    const normalized = path.trim() || DEFAULT_TEST_BRACKET_PATH;
    setSelectedBracketPath(normalized);
    const nextConfig = { ...config, testBracketPath: normalized };
    setConfig(nextConfig);
    await saveConfig(nextConfig);
    await resetBracketState(normalized);
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

  function columnLabel(round: number, isLosers: boolean) {
    if (round === 0) {
      return "GF";
    }
    const label = Math.abs(round);
    return isLosers ? `L${label}` : `W${label}`;
  }

  function columnHeight(baseCount: number): string | undefined {
    if (!baseCount) {
      return undefined;
    }
    return `${baseCount * BRACKET_SET_HEIGHT}px`;
  }

  function formatSetState(state: string) {
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

  async function refreshBracketState() {
    setBracketStatus("");
    try {
      const res = await invoke("startgg_sim_raw_state");
      const normalized = normalizeStartggResponse(res);
      if (normalized) {
        applyNormalizedState(normalized);
      }
    } catch (e) {
      const msg =
        e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setBracketStatus(`Bracket update failed: ${msg}`);
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

  function renderBracketSet(set: StartggSimState["sets"][number]) {
    const hasBothEntrants = set.slots.every(
      (slot) => slot.entrantId !== null && slot.entrantId !== undefined,
    );
    const isByeSet = !hasBothEntrants;
    const isDqSet = set.slots.some((slot) => slot.result === "dq");
    const hasReplay = !isByeSet && !isDqSet && replaySet.has(set.id);
    const stateLabel = isByeSet ? "Bye" : formatSetState(set.state);
    const stateData = isByeSet ? "bye" : set.state;
    const setClassName = [
      "bracket-set",
      isByeSet ? "bye" : "",
      isDqSet ? "dq" : "",
      hasReplay ? "has-replay" : "",
    ]
      .filter(Boolean)
      .join(" ");

    return (
      <div key={set.id} className={setClassName}>
        <div className="bracket-set-header">
          <div className="bracket-set-meta">
            <span className="state-pill" data-state={stateData}>
              {stateLabel}
            </span>
            {hasReplay && (
              <button
                type="button"
                className="replay-pill"
                onClick={() => streamBracketReplay(set.id)}
              >
                Replay
              </button>
            )}
          </div>
        </div>
        {set.slots.map((slot, idx) => {
          const slotLabel = slot.entrantName ?? "Bye";
          const scoreLabel = slot.result === "dq" ? "DQ" : slot.score;
          const showScore = scoreLabel !== null && scoreLabel !== undefined && scoreLabel !== "";
          return (
            <div
              key={`${set.id}-slot-${idx}`}
              className={[
                "bracket-slot",
                slot.result === "win" ? "winner" : "",
                slot.result === "loss" ? "loser" : "",
                slot.result === "dq" ? "dq" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              <span className="bracket-slot-name">{slotLabel}</span>
              {showScore && <span className="bracket-slot-score">{scoreLabel}</span>}
            </div>
          );
        })}
      </div>
    );
  }

  const replayStreamSet = useMemo(() => {
    if (!bracketState || !replayStreamUpdate?.setId) {
      return null;
    }
    return bracketState.sets.find((set) => set.id === replayStreamUpdate.setId) ?? null;
  }, [bracketState, replayStreamUpdate?.setId]);

  const replayStatus = (() => {
    if (!replayStreamUpdate) {
      return { label: "Idle", tone: "idle" };
    }
    switch (replayStreamUpdate.type) {
      case "start":
      case "progress":
        return { label: "Streaming", tone: "streaming" };
      case "complete":
        return { label: "Complete", tone: "complete" };
      case "error":
        return { label: "Error", tone: "error" };
      default:
        return { label: replayStreamUpdate.type, tone: "idle" };
    }
  })();

  const replaySetLabel = replayStreamSet
    ? `${replayStreamSet.slots[0]?.entrantName ?? "Bye"} vs ${replayStreamSet.slots[1]?.entrantName ?? "Bye"}`
    : replayStreamUpdate?.setId
      ? `Set ${replayStreamUpdate.setId}`
      : "N/A";
  const replayRoundLabel = replayStreamSet?.roundLabel ?? "";
  const replayPath = replayStreamUpdate?.replayPath ?? "";
  const replayName = replayPath ? replayPath.split(/[\\/]/).pop() ?? replayPath : "N/A";
  const frame = replayStreamUpdate?.frame;
  const totalFrames = replayStreamUpdate?.totalFrames;
  const frameLabel =
    frame !== null && frame !== undefined
      ? totalFrames !== null && totalFrames !== undefined
        ? `${frame} / ${totalFrames}`
        : `${frame}`
      : "N/A";
  const progressPct =
    frame !== null && frame !== undefined && totalFrames && totalFrames > 0
      ? Math.min(100, Math.max(0, Math.round((frame / totalFrames) * 100)))
      : null;
  const replayIndex = replayStreamUpdate?.replayIndex;
  const replayTotal = replayStreamUpdate?.replayTotal;
  const gameLabel =
    replayIndex && replayTotal ? `Game ${replayIndex} of ${replayTotal}` : "N/A";
  const elapsedMs = replayStreamStartedAt ? Date.now() - replayStreamStartedAt : null;
  const elapsedLabel =
    elapsedMs !== null ? `${Math.max(0, Math.floor(elapsedMs / 1000))}s` : "N/A";

  return (
    <>
      {isBracketView ? (
        <main className="app bracket-app">
          <div className="section-header">
            <div>
              <p className="eyebrow">Test Mode</p>
              <h2>{bracketState?.event.name ?? "Start.gg Test Bracket"}</h2>
              <p className="muted tiny">
                {bracketState?.phases[0]?.name ?? "Singles Bracket"} ·{" "}
                {bracketState ? `${bracketState.entrants.length} entrants` : "Loading…"}
              </p>
              {bracketState?.eventLink && (
                <button
                  type="button"
                  className="event-link"
                  onClick={() => openEventLink(bracketState.eventLink ?? "")}
                >
                  Event link
                </button>
              )}
            </div>
            <div className="action-row">
              <button className="ghost-btn small" onClick={completeBracket}>
                Auto-complete
              </button>
              <button className="icon-button" onClick={openBracketSettings} aria-label="Open bracket settings">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M12 8.6a3.4 3.4 0 1 0 0 6.8 3.4 3.4 0 0 0 0-6.8zm9 3.4c0-.5-.04-1-.12-1.48l2.12-1.65-2-3.46-2.54 1a8.7 8.7 0 0 0-2.56-1.48l-.38-2.7h-4l-.38 2.7c-.9.28-1.76.76-2.56 1.48l-2.54-1-2 3.46 2.12 1.65c-.08.48-.12.98-.12 1.48s.04 1 .12 1.48L1.94 14.99l2 3.46 2.54-1a8.7 8.7 0 0 0 2.56 1.48l.38 2.7h4l.38-2.7c.9-.28 1.76-.76 2.56-1.48l2.54 1 2-3.46-2.12-1.65c.08-.48.12-.98.12-1.48zM12 17.2a5.2 5.2 0 1 1 0-10.4 5.2 5.2 0 0 1 0 10.4z" />
                </svg>
              </button>
            </div>
          </div>
          {bracketStatus && <div className="status-line">{bracketStatus}</div>}

          <section className="panel">
            <div className="section-header">
              <div>
                <p className="eyebrow">Replay Stream</p>
              </div>
              <div className="streaming-badge" data-status={replayStatus.tone}>
                {replayStatus.label}
              </div>
            </div>
            {!replayStreamUpdate && (
              <div className="todo-box">No replay streaming yet.</div>
            )}
            {replayStreamUpdate && (
              <>
                <div className="streaming-grid">
                  <div className="streaming-field">
                    <div className="label">Set</div>
                    <div className="value">{replaySetLabel}</div>
                    {replayRoundLabel && <div className="muted tiny">{replayRoundLabel}</div>}
                  </div>
                  <div className="streaming-field">
                    <div className="label">Replay</div>
                    <div className="value">{replayName}</div>
                    <div className="muted tiny code">{replayPath || "N/A"}</div>
                  </div>
                  <div className="streaming-field">
                    <div className="label">Frames</div>
                    <div className="value">{frameLabel}</div>
                    <div className="muted tiny">Elapsed {elapsedLabel}</div>
                  </div>
                  <div className="streaming-field">
                    <div className="label">Queue</div>
                    <div className="value">{gameLabel}</div>
                    <div className="muted tiny">Output {config.spectateFolderPath || "N/A"}</div>
                  </div>
                </div>
                {progressPct !== null && (
                  <div className="streaming-progress">
                    <div className="streaming-progress-bar" style={{ width: `${progressPct}%` }} />
                  </div>
                )}
                {replayStreamUpdate.type === "error" && replayStreamUpdate.message && (
                  <div className="status-line warning">{replayStreamUpdate.message}</div>
                )}
              </>
            )}
          </section>

          <section className="panel">
            <div className="section-header">
              <div>
                <p className="eyebrow">Bracket</p>
              </div>
            </div>
            {!bracketRounds && <div className="todo-box">Loading bracket…</div>}
            {bracketRounds && (
              <div className="bracket-zoom-frame">
                <div
                  className="bracket-zoom"
                  style={
                    {
                      "--bracket-scale": bracketZoom,
                      "--bracket-set-height": `${BRACKET_SET_HEIGHT}px`,
                    } as CSSProperties
                  }
                >
                  <div className="bracket-layout">
                    <div className="bracket-section">
                      <div className="bracket-section-title">Winners</div>
                      <div
                        className="bracket-columns"
                        style={
                          {
                            "--bracket-column-height": columnHeight(bracketRounds.winnersBase),
                          } as CSSProperties
                        }
                      >
                        {bracketRounds.winnersRounds.map(([round, sets]) => (
                          <div key={`w-${round}`} className="bracket-column">
                            <div className="bracket-column-title">{columnLabel(round, false)}</div>
                            {sets.map((set) => renderBracketSet(set))}
                          </div>
                        ))}
                      </div>
                    </div>

                    <div className="bracket-section">
                      <div className="bracket-section-title">Losers</div>
                      <div
                        className="bracket-columns"
                        style={
                          {
                            "--bracket-column-height": columnHeight(bracketRounds.losersBase),
                          } as CSSProperties
                        }
                      >
                        {bracketRounds.losersRounds.map(([round, sets]) => (
                          <div key={`l-${round}`} className="bracket-column">
                            <div className="bracket-column-title">{columnLabel(round, true)}</div>
                            {sets.map((set) => renderBracketSet(set))}
                          </div>
                        ))}
                      </div>
                    </div>

                  </div>
                </div>
              </div>
            )}
          </section>
        </main>
      ) : (
        <main className="app">
        <div className="top-bar">
          {config.testMode && (
            <button className="ghost-btn small" onClick={openBracketWindow}>
              Spoof bracket
            </button>
          )}
          <button className="icon-button" onClick={openSettings} aria-label="Open settings">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 8.6a3.4 3.4 0 1 0 0 6.8 3.4 3.4 0 0 0 0-6.8zm9 3.4c0-.5-.04-1-.12-1.48l2.12-1.65-2-3.46-2.54 1a8.7 8.7 0 0 0-2.56-1.48l-.38-2.7h-4l-.38 2.7c-.9.28-1.76.76-2.56 1.48l-2.54-1-2 3.46 2.12 1.65c-.08.48-.12.98-.12 1.48s.04 1 .12 1.48L1.94 14.99l2 3.46 2.54-1a8.7 8.7 0 0 0 2.56 1.48l.38 2.7h4l.38-2.7c.9-.28 1.76-.76 2.56-1.48l2.54 1 2-3.46-2.12-1.65c.08-.48.12-.98.12-1.48zM12 17.2a5.2 5.2 0 1 1 0-10.4 5.2 5.2 0 0 1 0 10.4z" />
            </svg>
          </button>
        </div>
        {topStatus && <div className="status-line">{topStatus}</div>}
        <section className="panel">
          <div className="section-header">
            <div>
              <p className="eyebrow">Setups</p>
            </div>
            <div className="action-row">
              <button className="ghost-btn" onClick={addSetup}>
                Add setup
              </button>
            </div>
          </div>
          {setupStatus && <div className="status-line">{setupStatus}</div>}
          <div className="setup-grid">
            {setups.length === 0 ? (
              <div className="todo-box">No setups yet. Click Add setup to get started.</div>
            ) : (
              setups.map((s) => {
                const assigned = s.assignedStream;
                const left = assigned?.p1Tag ?? assigned?.p1Code ?? "Unknown";
                const right = assigned?.p2Tag ?? assigned?.p2Code;
                const label = assigned ? (right ? `${left} vs ${right}` : left) : "Unassigned";
                return (
                  <article key={s.id} className="setup-card">
                    <header>
                      <div className="setup-name">{s.name}</div>
                      <div className="setup-actions">
                        {assigned && (
                          <button className="ghost-btn small" onClick={() => clearSetup(s.id)}>
                            Clear
                          </button>
                        )}
                        <button className="ghost-btn small" onClick={() => removeSetup(s.id)}>
                          Delete
                        </button>
                      </div>
                    </header>
                    <div className="muted">Assigned: {label}</div>
                    {assigned && <div className="muted code">Stream: {assigned.id}</div>}
                    <div className="muted">Dolphin label: dolphin-{s.id}</div>
                  </article>
                );
              })
            )}
          </div>
        </section>

        <section className="panel">
          <div className="section-header">
            <div>
              <p className="eyebrow">Detection</p>
            </div>
            <div className="action-row">
              <button className="ghost-btn" onClick={launchSlippi}>
                Launch Slippi
              </button>
              <button className="ghost-btn" onClick={launchDolphin}>
                Launch Dolphin
              </button>
              <button className="ghost-btn" onClick={refreshSlippiThenScan}>
                Refresh
              </button>
            </div>
          </div>
          {launchStatus && <div className="status-line">{launchStatus}</div>}
          {dolphinLaunchStatus && <div className="status-line">{dolphinLaunchStatus}</div>}
          {slippiRefreshStatus && <div className="status-line">{slippiRefreshStatus}</div>}
          {watchStatus && <div className="status-line">{watchStatus}</div>}
          {windowStatus && <div className="status-line warning">{windowStatus}</div>}
          <div className="status-line">{streamsStatus}</div>

          <div className="streams-grid">
            {streams.length === 0 ? (
              <div className="todo-box">No live streams detected. Keep Slippi running and hit Refresh.</div>
            ) : (
              streams.map((s) => (
                <article key={s.id} className="stream-card">
                  <div className="stream-row">
                    <div className="stream-pair">
                      <div>
                        <div className="value">{s.p1Tag ?? "Unknown"}</div>
                        <div className="muted code">{s.p1Code ?? "N/A"}</div>
                      </div>
                    </div>
                    <div className="stream-actions">
                      <select
                        className="ghost-select"
                        value={getStreamSetupId(s.id) ?? ""}
                        onChange={(e) => handleSetupSelect(s.id, e.target.value)}
                      >
                        <option value="">Choose setup…</option>
                        {setups.map((setup) => (
                          <option key={setup.id} value={setup.id}>
                            {setup.name}
                          </option>
                        ))}
                      </select>
                      <button className="ghost-btn small" onClick={() => watchStream(s)}>
                        Watch
                      </button>
                    </div>
                  </div>
                </article>
              ))
            )}
          </div>
        </section>
      </main>
      )}

      {isBracketView && bracketSettingsOpen && (
        <div className="modal-backdrop" onClick={closeBracketSettings}>
          <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Bracket settings">
            <div className="modal-header">
              <div>
                <p className="eyebrow">Bracket Settings</p>
              </div>
              <button className="icon-button" onClick={closeBracketSettings} aria-label="Close bracket settings">
                x
              </button>
            </div>
            <div className="settings-grid">
              <div className="settings-toggle">
                <div className="settings-label">Bracket controls</div>
                <div className="action-row">
                  <button className="ghost-btn small" onClick={resetBracketState}>
                    Reset bracket
                  </button>
                  <button className="ghost-btn small" onClick={() => refreshBracketState()}>
                    Refresh
                  </button>
                </div>
              </div>
              <div className="settings-toggle">
                <div className="settings-label">Auto-complete on reset</div>
                <label className="settings-checkbox">
                  <input
                    type="checkbox"
                    checked={config.autoCompleteBracket}
                    onChange={(e) => setAutoCompleteBracket(e.target.checked)}
                  />
                  <span>Enabled</span>
                </label>
              </div>
              <label className="settings-field">
                <span>Bracket zoom</span>
                <div className="range-row">
                  <input
                    type="range"
                    min={BRACKET_ZOOM_MIN}
                    max={BRACKET_ZOOM_MAX}
                    step={BRACKET_ZOOM_STEP}
                    value={bracketZoom}
                    onChange={(e) => setBracketZoom(Number(e.target.value))}
                  />
                  <div className="range-value">{Math.round(bracketZoom * 100)}%</div>
                </div>
              </label>
            </div>
          </div>
        </div>
      )}

      {!isBracketView && settingsOpen && (
        <div className="modal-backdrop" onClick={closeSettings}>
          <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Settings">
            <div className="modal-header">
              <div>
                <p className="eyebrow">Settings</p>
              </div>
              <button className="icon-button" onClick={closeSettings} aria-label="Close settings">
                x
              </button>
            </div>
            <div className="settings-grid">
              <div className="settings-toggle">
                <div className="settings-label">Test mode</div>
                <button className="ghost-btn small" onClick={toggleTestMode}>
                  {config.testMode ? "Disable" : "Enable"}
                </button>
              </div>
              {config.testMode && (
                <label className="settings-field">
                  <span>Test bracket</span>
                  <select
                    className="ghost-select"
                    value={selectedBracketPath}
                    onChange={(e) => handleBracketSelect(e.target.value)}
                  >
                    <option value="">Select bracket…</option>
                    {bracketConfigs.map((config) => (
                      <option key={config.path} value={config.path}>
                        {config.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <label className="settings-field">
                <span>Dolphin path</span>
                <div className="path-input">
                  <input
                    type="text"
                    value={config.dolphinPath}
                    onChange={(e) => updateConfig("dolphinPath", e.target.value)}
                    placeholder="/path/to/dolphin"
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="icon-button small"
                    onClick={() => browsePath("dolphinPath", { directory: false, title: "Select Dolphin binary" })}
                    aria-label="Browse for Dolphin path"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M10 4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h6z" />
                    </svg>
                  </button>
                </div>
              </label>
              <label className="settings-field">
                <span>Melee ISO path</span>
                <div className="path-input">
                  <input
                    type="text"
                    value={config.ssbmIsoPath}
                    onChange={(e) => updateConfig("ssbmIsoPath", e.target.value)}
                    placeholder="/path/to/melee.iso"
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="icon-button small"
                    onClick={() => browsePath("ssbmIsoPath", { directory: false, title: "Select Melee ISO" })}
                    aria-label="Browse for Melee ISO path"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M10 4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h6z" />
                    </svg>
                  </button>
                </div>
              </label>
              <label className="settings-field">
                <span>Slippi launcher path</span>
                <div className="path-input">
                  <input
                    type="text"
                    value={config.slippiLauncherPath}
                    onChange={(e) => updateConfig("slippiLauncherPath", e.target.value)}
                    placeholder="/path/to/slippi.AppImage"
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="icon-button small"
                    onClick={() => browsePath("slippiLauncherPath", { directory: false, title: "Select Slippi Launcher" })}
                    aria-label="Browse for Slippi Launcher path"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M10 4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h6z" />
                    </svg>
                  </button>
                </div>
              </label>
              <label className="settings-field">
                <span>Spectate folder path</span>
                <div className="path-input">
                  <input
                    type="text"
                    value={config.spectateFolderPath}
                    onChange={(e) => updateConfig("spectateFolderPath", e.target.value)}
                    placeholder="/path/to/spectate"
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="icon-button small"
                    onClick={() => browsePath("spectateFolderPath", { directory: true, title: "Select Spectate folder" })}
                    aria-label="Browse for Spectate folder"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M10 4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h6z" />
                    </svg>
                  </button>
                </div>
              </label>
            </div>
            <div className="modal-actions">
              {configStatus && <div className="modal-status">{configStatus}</div>}
              <button className="ghost-btn" onClick={saveConfig}>
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
