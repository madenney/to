import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  AppConfig,
  BracketConfigInfo,
  StartggSimState,
  StartggLiveSnapshot,
} from "../types/overlay";
import { normalizeStartggResponse } from "../startggAdapter";

const DEFAULT_TEST_BRACKET_PATH = "test_brackets/test_bracket_2.json";

export type UseConfigReturn = {
  config: AppConfig;
  setConfig: React.Dispatch<React.SetStateAction<AppConfig>>;
  settingsOpen: boolean;
  configStatus: string;
  bracketConfigs: BracketConfigInfo[];
  selectedBracketPath: string;
  setSelectedBracketPath: React.Dispatch<React.SetStateAction<string>>;
  pendingStartggFocus: "link" | "token" | null;
  startggLinkInputRef: React.RefObject<HTMLInputElement | null>;
  startggTokenInputRef: React.RefObject<HTMLInputElement | null>;
  startggLinkProvided: boolean;
  needsStartggLink: boolean;
  needsStartggToken: boolean;
  startggTokenError: boolean;
  startggStatus: { text: string; tone: string } | null;
  liveStartggState: StartggSimState | null;
  testStartggState: StartggSimState | null;
  setTestStartggState: React.Dispatch<React.SetStateAction<StartggSimState | null>>;
  startggLiveError: string;
  startggLiveLoading: boolean;
  startggPollLoading: boolean;
  currentStartggState: StartggSimState | null;
  updateConfig: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  loadConfig: () => Promise<AppConfig | null>;
  saveConfig: (nextConfig?: AppConfig) => Promise<void>;
  toggleTestMode: () => Promise<void>;
  setAutoCompleteBracket: (enabled: boolean) => Promise<void>;
  openSettings: (options?: { focusStartgg?: "link" | "token" }) => void;
  closeSettings: () => void;
  browsePath: (key: keyof AppConfig, options: { directory: boolean; title: string }) => Promise<void>;
  refreshTestStartggState: () => Promise<StartggSimState | null>;
  refreshLiveStartggState: (force?: boolean) => Promise<StartggSimState | null>;
  pollStartggCycle: () => Promise<void>;
  loadBracketConfigs: () => Promise<void>;
  handleBracketSelect: (path: string) => Promise<void>;
  /** Callback refs — set these after all hooks are initialized to break circular deps */
  resetBracketStateRef: React.MutableRefObject<((pathOverride?: string, autoCompleteOverride?: boolean) => Promise<void>) | null>;
  setTopStatusRef: React.MutableRefObject<((status: string) => void) | null>;
  setBracketStatusRef: React.MutableRefObject<((status: string) => void) | null>;
};

export function useConfig(
  isBracketView: boolean,
): UseConfigReturn {
  const [config, setConfig] = useState<AppConfig>({
    dolphinPath: "",
    ssbmIsoPath: "",
    slippiLauncherPath: "",
    spectateFolderPath: "",
    startggLink: "",
    startggToken: "",
    startggPolling: false,
    autoStream: true,
    testMode: false,
    testBracketPath: DEFAULT_TEST_BRACKET_PATH,
    autoCompleteBracket: true,
  });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [configStatus, setConfigStatus] = useState("");
  const [bracketConfigs, setBracketConfigs] = useState<BracketConfigInfo[]>([]);
  const [selectedBracketPath, setSelectedBracketPath] = useState(DEFAULT_TEST_BRACKET_PATH);
  const [pendingStartggFocus, setPendingStartggFocus] = useState<"link" | "token" | null>(null);
  const startggLinkInputRef = useRef<HTMLInputElement | null>(null);
  const startggTokenInputRef = useRef<HTMLInputElement | null>(null);

  const [liveStartggState, setLiveStartggState] = useState<StartggSimState | null>(null);
  const [testStartggState, setTestStartggState] = useState<StartggSimState | null>(null);
  const [startggLiveError, setStartggLiveError] = useState("");
  const [startggLiveLoading, setStartggLiveLoading] = useState(false);
  const [startggPollLoading, setStartggPollLoading] = useState(false);
  const startggPollInFlight = useRef(false);

  // Callback refs to break circular dependency with useBracket/useStreams
  const resetBracketStateRef = useRef<((pathOverride?: string, autoCompleteOverride?: boolean) => Promise<void>) | null>(null);
  const setTopStatusRef = useRef<((status: string) => void) | null>(null);
  const setBracketStatusRef = useRef<((status: string) => void) | null>(null);

  const currentStartggState = useMemo(
    () => (config.testMode ? testStartggState : liveStartggState),
    [config.testMode, testStartggState, liveStartggState],
  );

  const startggLinkProvided = config.startggLink.trim().length > 0;
  const needsStartggLink = !config.testMode && !startggLinkProvided;
  const needsStartggToken = !config.testMode && startggLinkProvided && !config.startggToken.trim();
  const startggTokenError = !config.testMode && startggLinkProvided && Boolean(startggLiveError);

  const startggStatus = useMemo(() => {
    if (config.testMode) {
      return null;
    }
    if (!startggLinkProvided) {
      return { text: "Enter a Start.gg link to load live data.", tone: "muted" };
    }
    if (startggLiveError) {
      return { text: startggLiveError, tone: "warning" };
    }
    if (!liveStartggState) {
      return {
        text: startggLiveLoading ? "Fetching Start.gg data..." : "Waiting for Start.gg data.",
        tone: "muted",
      };
    }
    const name = liveStartggState.event?.name?.trim() || "Start.gg event";
    const entrants = liveStartggState.entrants?.length ?? 0;
    return { text: `Connected: ${name} (${entrants} entrants)`, tone: "ok" };
  }, [config.testMode, startggLinkProvided, startggLiveError, liveStartggState, startggLiveLoading]);

  function updateConfig<K extends keyof AppConfig>(key: K, value: AppConfig[K]) {
    setConfig((prev) => ({ ...prev, [key]: value }));
  }

  async function loadConfig(): Promise<AppConfig | null> {
    try {
      const res = await invoke<AppConfig>("load_config");
      const bracketPath = (res.testBracketPath ?? "").trim() || DEFAULT_TEST_BRACKET_PATH;
      const nextConfig = {
        ...res,
        startggLink: res.startggLink ?? "",
        startggToken: res.startggToken ?? "",
        startggPolling: res.startggPolling ?? false,
        autoStream: res.autoStream ?? true,
        testMode: res.testMode ?? false,
        autoCompleteBracket: res.autoCompleteBracket ?? true,
        testBracketPath: bracketPath,
      };
      setConfig(nextConfig);
      setSelectedBracketPath(bracketPath);
      setConfigStatus("");
      return nextConfig;
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
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
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
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

  function openSettings(options?: { focusStartgg?: "link" | "token" }) {
    setSettingsOpen(true);
    setConfigStatus("");
    if (options?.focusStartgg) {
      setPendingStartggFocus(options.focusStartgg);
    }
    loadConfig();
  }

  function closeSettings() {
    setSettingsOpen(false);
    setConfigStatus("");
    setPendingStartggFocus(null);
  }

  async function browsePath(key: keyof AppConfig, options: { directory: boolean; title: string }) {
    try {
      const current = (config[key] as string).trim();
      const selected = await openDialog({
        directory: options.directory,
        multiple: false,
        title: options.title,
        defaultPath: current || undefined,
      });
      if (typeof selected === "string") {
        updateConfig(key, selected as AppConfig[typeof key]);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setConfigStatus(`Open dialog failed: ${msg}`);
    }
  }

  async function refreshTestStartggState(): Promise<StartggSimState | null> {
    try {
      const res = await invoke("startgg_sim_raw_state");
      const normalized = normalizeStartggResponse(res);
      setTestStartggState(normalized);
      return normalized;
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setTopStatusRef.current?.(`Test bracket update failed: ${msg}`);
      return null;
    }
  }

  async function refreshLiveStartggState(force = false): Promise<StartggSimState | null> {
    setStartggLiveLoading(true);
    try {
      const snapshot = await invoke<StartggLiveSnapshot>("startgg_live_snapshot", { force });
      setLiveStartggState(snapshot.state ?? null);
      setStartggLiveError(snapshot.lastError ?? "");
      return snapshot.state ?? null;
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setStartggLiveError(`Start.gg snapshot failed: ${msg}`);
      return null;
    } finally {
      setStartggLiveLoading(false);
    }
  }

  async function pollStartggCycle() {
    if (startggPollInFlight.current) {
      return;
    }
    startggPollInFlight.current = true;
    setStartggPollLoading(true);
    try {
      if (config.testMode) {
        await refreshTestStartggState();
        return;
      }
      if (!startggLinkProvided) {
        setTopStatusRef.current?.("Add a Start.gg event link in Settings before polling.");
        return;
      }
      await refreshLiveStartggState(true);
    } finally {
      startggPollInFlight.current = false;
      setStartggPollLoading(false);
    }
  }

  async function loadBracketConfigs() {
    try {
      const res = await invoke<BracketConfigInfo[]>("list_bracket_configs");
      setBracketConfigs(res);
      if (selectedBracketPath && !res.some((c) => c.path === selectedBracketPath)) {
        setSelectedBracketPath("");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      if (isBracketView) {
        setBracketStatusRef.current?.(`Load bracket configs failed: ${msg}`);
      } else {
        setConfigStatus(`Load bracket configs failed: ${msg}`);
      }
    }
  }

  async function handleBracketSelect(path: string) {
    const normalized = path.trim() || DEFAULT_TEST_BRACKET_PATH;
    setSelectedBracketPath(normalized);
    const nextConfig = { ...config, testBracketPath: normalized };
    setConfig(nextConfig);
    await saveConfig(nextConfig);
    await resetBracketStateRef.current?.(normalized);
  }

  // Auto-focus start.gg inputs
  useEffect(() => {
    if (!settingsOpen || !pendingStartggFocus) {
      return;
    }
    const timer = window.setTimeout(() => {
      if (pendingStartggFocus === "link" && startggLinkInputRef.current) {
        startggLinkInputRef.current.focus();
      } else if (pendingStartggFocus === "token" && startggTokenInputRef.current) {
        startggTokenInputRef.current.focus();
      }
      setPendingStartggFocus(null);
    }, 100);
    return () => window.clearTimeout(timer);
  }, [settingsOpen, pendingStartggFocus]);

  // Live start.gg polling
  useEffect(() => {
    if (isBracketView || config.testMode || !startggLinkProvided) {
      setLiveStartggState(null);
      setStartggLiveError("");
      return;
    }
    let cancelled = false;
    let timer: number | null = null;
    const refreshLive = async () => {
      if (cancelled) return;
      await refreshLiveStartggState();
    };
    refreshLive();
    if (config.startggPolling) {
      timer = window.setInterval(refreshLive, 5000);
    }
    return () => {
      cancelled = true;
      if (timer) window.clearInterval(timer);
    };
  }, [isBracketView, config.testMode, startggLinkProvided, config.startggLink, config.startggToken, config.startggPolling]);

  // Test start.gg polling
  useEffect(() => {
    if (isBracketView || !config.testMode || !config.startggPolling) {
      return;
    }
    let cancelled = false;
    let timer: number | null = null;
    const refreshTest = async () => {
      if (cancelled) return;
      await refreshTestStartggState();
    };
    refreshTest();
    timer = window.setInterval(refreshTest, 2500);
    return () => {
      cancelled = true;
      if (timer) window.clearInterval(timer);
    };
  }, [isBracketView, config.testMode, config.startggPolling, config.testBracketPath]);

  // Load bracket configs when settings open in test mode
  useEffect(() => {
    if (isBracketView || !settingsOpen || !config.testMode) {
      return;
    }
    loadBracketConfigs();
  }, [isBracketView, settingsOpen, config.testMode]);

  return {
    config,
    setConfig,
    settingsOpen,
    configStatus,
    bracketConfigs,
    selectedBracketPath,
    setSelectedBracketPath,
    pendingStartggFocus,
    startggLinkInputRef,
    startggTokenInputRef,
    startggLinkProvided,
    needsStartggLink,
    needsStartggToken,
    startggTokenError,
    startggStatus,
    liveStartggState,
    testStartggState,
    setTestStartggState,
    startggLiveError,
    startggLiveLoading,
    startggPollLoading,
    currentStartggState,
    updateConfig,
    loadConfig,
    saveConfig,
    toggleTestMode,
    setAutoCompleteBracket,
    openSettings,
    closeSettings,
    browsePath,
    refreshTestStartggState,
    refreshLiveStartggState,
    pollStartggCycle,
    loadBracketConfigs,
    handleBracketSelect,
    resetBracketStateRef,
    setTopStatusRef,
    setBracketStatusRef,
  };
}
