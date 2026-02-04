import { useState, useRef, useMemo, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Setup } from "../types/overlay";

const MAX_SETUPS = 16;
const SETUP_STATUS_TIMEOUT_MS = 2500;

export type UseSetupsReturn = {
  setups: Setup[];
  setSetups: React.Dispatch<React.SetStateAction<Setup[]>>;
  setupStatus: string;
  setupDetailsId: number | null;
  setupDetails: Setup | null;
  setupDetailsJson: string;
  overlayCopyStatus: string;
  setupOverlayUrl: string;
  loadSetups: () => Promise<void>;
  addSetup: () => Promise<void>;
  removeSetup: (id: number) => Promise<void>;
  removeLastSetup: () => Promise<void>;
  openSetupDetails: (setupId: number) => void;
  closeSetupDetails: () => void;
  clearSetupAssignment: (
    id: number,
    assignedStreamId?: string,
    options?: { silent?: boolean; stop?: boolean },
  ) => Promise<boolean>;
  clearSetup: (id: number) => Promise<void>;
  setEphemeralSetupStatus: (message: string, timeoutMs?: number) => void;
  setPersistentSetupStatus: (message: string) => void;
  copyOverlayUrl: (value: string) => Promise<void>;
  autoManagedSetupIds: React.MutableRefObject<Set<number>>;
  streamSetupSelections: Record<string, number>;
  setStreamSetupSelections: React.Dispatch<React.SetStateAction<Record<string, number>>>;
};

export function useSetups(isBracketView: boolean): UseSetupsReturn {
  const [setups, setSetups] = useState<Setup[]>([]);
  const [setupStatus, setSetupStatus] = useState("No setups yet. Use + to start.");
  const [setupDetailsId, setSetupDetailsId] = useState<number | null>(null);
  const [overlayCopyStatus, setOverlayCopyStatus] = useState("");
  const [streamSetupSelections, setStreamSetupSelections] = useState<Record<string, number>>({});
  const setupStatusTimer = useRef<number | null>(null);
  const overlayCopyTimer = useRef<number | null>(null);
  const autoManagedSetupIds = useRef<Set<number>>(new Set());

  const setupDetails = useMemo(
    () => setups.find((setup) => setup.id === setupDetailsId) ?? null,
    [setups, setupDetailsId],
  );

  const setupDetailsJson = useMemo(
    () => (setupDetails ? JSON.stringify(setupDetails, null, 2) : ""),
    [setupDetails],
  );

  const setupOverlayUrl = useMemo(() => {
    if (!setupDetails) return "";
    return `http://127.0.0.1:17890/?setup=${setupDetails.id}`;
  }, [setupDetails]);

  function clearSetupStatusTimer() {
    if (setupStatusTimer.current) {
      window.clearTimeout(setupStatusTimer.current);
      setupStatusTimer.current = null;
    }
  }

  function setPersistentSetupStatus(message: string) {
    clearSetupStatusTimer();
    setSetupStatus(message);
  }

  function setEphemeralSetupStatus(message: string, timeoutMs = SETUP_STATUS_TIMEOUT_MS) {
    clearSetupStatusTimer();
    setSetupStatus(message);
    setupStatusTimer.current = window.setTimeout(() => {
      setSetupStatus("");
      setupStatusTimer.current = null;
    }, timeoutMs);
  }

  function clearOverlayCopyTimer() {
    if (overlayCopyTimer.current) {
      window.clearTimeout(overlayCopyTimer.current);
      overlayCopyTimer.current = null;
    }
  }

  function setEphemeralOverlayCopyStatus(message: string, timeoutMs = 1800) {
    clearOverlayCopyTimer();
    setOverlayCopyStatus(message);
    overlayCopyTimer.current = window.setTimeout(() => {
      setOverlayCopyStatus("");
      overlayCopyTimer.current = null;
    }, timeoutMs);
  }

  async function copyOverlayUrl(value: string) {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      setEphemeralOverlayCopyStatus("Copied!");
    } catch {
      try {
        const textarea = document.createElement("textarea");
        textarea.value = value;
        textarea.style.position = "fixed";
        textarea.style.left = "-9999px";
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        document.body.removeChild(textarea);
        setEphemeralOverlayCopyStatus("Copied!");
      } catch {
        setEphemeralOverlayCopyStatus("Copy failed.");
      }
    }
  }

  function openSetupDetails(setupId: number) {
    clearOverlayCopyTimer();
    setOverlayCopyStatus("");
    setSetupDetailsId(setupId);
  }

  function closeSetupDetails() {
    clearOverlayCopyTimer();
    setOverlayCopyStatus("");
    setSetupDetailsId(null);
  }

  async function loadSetups() {
    try {
      const res = await invoke<Setup[]>("list_setups");
      setSetups(res);
      setPersistentSetupStatus(res.length === 0 ? "No setups yet. Use + to start." : "");
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setPersistentSetupStatus(`Load setups failed: ${msg}`);
    }
  }

  async function addSetup() {
    if (setups.length >= MAX_SETUPS) {
      setPersistentSetupStatus(`Max setups (${MAX_SETUPS}) reached.`);
      return;
    }
    setPersistentSetupStatus("Creating setup…");
    try {
      const setup = await invoke<Setup>("create_setup");
      setSetups((prev) => {
        const next = [...prev, setup];
        next.sort((a, b) => a.id - b.id);
        return next;
      });
      setEphemeralSetupStatus(`Setup ${setup.id} created.`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setPersistentSetupStatus(`Add setup failed: ${msg}`);
    }
  }

  async function removeSetup(id: number) {
    setPersistentSetupStatus(`Deleting setup ${id}…`);
    try {
      await invoke("delete_setup", { id });
      setSetups((prev) => {
        const next = prev.filter((s) => s.id !== id);
        next.sort((a, b) => a.id - b.id);
        return next;
      });
      setEphemeralSetupStatus(`Setup ${id} deleted.`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setPersistentSetupStatus(`Delete setup failed: ${msg}`);
    }
  }

  async function removeLastSetup() {
    if (setups.length === 0) {
      setPersistentSetupStatus("No setups to remove.");
      return;
    }
    const target = setups[setups.length - 1];
    await removeSetup(target.id);
  }

  async function clearSetupAssignment(
    id: number,
    assignedStreamId?: string,
    options: { silent?: boolean; stop?: boolean } = {},
  ): Promise<boolean> {
    const silent = options.silent ?? false;
    const stop = options.stop ?? true;
    if (!silent) {
      setPersistentSetupStatus(`Clearing setup ${id}…`);
    }
    try {
      const updated = await invoke<Setup>("clear_setup_assignment", { setupId: id, stop });
      setSetups((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      autoManagedSetupIds.current.delete(id);
      if (assignedStreamId) {
        setStreamSetupSelections((prev) => {
          const next = { ...prev };
          delete next[assignedStreamId];
          return next;
        });
      }
      if (!silent) {
        setEphemeralSetupStatus(`Setup ${id} cleared.`);
      }
      return true;
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      const prefix = silent ? `Auto-clear setup ${id} failed` : "Clear setup failed";
      setPersistentSetupStatus(`${prefix}: ${msg}`);
      return false;
    }
  }

  async function clearSetup(id: number) {
    const assignedStreamId = setups.find((s) => s.id === id)?.assignedStream?.id;
    await clearSetupAssignment(id, assignedStreamId);
  }

  // Cleanup timer on unmount
  useEffect(() => {
    return () => clearSetupStatusTimer();
  }, []);

  // Close details if setup removed
  useEffect(() => {
    if (setupDetailsId !== null && !setups.some((s) => s.id === setupDetailsId)) {
      closeSetupDetails();
    }
  }, [setups, setupDetailsId]);

  // Escape key to close details
  useEffect(() => {
    if (setupDetailsId === null) return;
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeSetupDetails();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [setupDetailsId]);

  // Sync stream setup selections when setups change
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

  return {
    setups,
    setSetups,
    setupStatus,
    setupDetailsId,
    setupDetails,
    setupDetailsJson,
    overlayCopyStatus,
    setupOverlayUrl,
    loadSetups,
    addSetup,
    removeSetup,
    removeLastSetup,
    openSetupDetails,
    closeSetupDetails,
    clearSetupAssignment,
    clearSetup,
    setEphemeralSetupStatus,
    setPersistentSetupStatus,
    copyOverlayUrl,
    autoManagedSetupIds,
    streamSetupSelections,
    setStreamSetupSelections,
  };
}
