import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { UnifiedEntrant, SetupWithSeed } from "../types/overlay";

export type UseEntrantsReturn = {
  entrants: UnifiedEntrant[];
  activeEntrants: UnifiedEntrant[];
  setupsWithSeed: SetupWithSeed[];
  autoAssignEnabled: boolean;
  entrantsStatus: string;
  selectedEntrantId: number | null;
  editingCodeFor: number | null;
  loadEntrants: () => Promise<void>;
  loadSetups: () => Promise<void>;
  syncFromStartgg: () => Promise<void>;
  setSlippiCode: (entrantId: number, code: string | null) => Promise<void>;
  assignToSetup: (entrantId: number, setupId: number | null) => Promise<void>;
  unassignEntrant: (entrantId: number) => Promise<void>;
  toggleAutoAssignment: (enabled: boolean) => Promise<void>;
  runAutoAssignment: () => Promise<void>;
  selectEntrant: (entrantId: number | null) => void;
  openCodeEditor: (entrantId: number) => void;
  closeCodeEditor: () => void;
  getEntrantById: (id: number) => UnifiedEntrant | undefined;
};

export function useEntrants(): UseEntrantsReturn {
  const [entrants, setEntrants] = useState<UnifiedEntrant[]>([]);
  const [setupsWithSeed, setSetupsWithSeed] = useState<SetupWithSeed[]>([]);
  const [autoAssignEnabled, setAutoAssignEnabled] = useState(false);
  const [entrantsStatus, setEntrantsStatus] = useState("");
  const [selectedEntrantId, setSelectedEntrantId] = useState<number | null>(null);
  const [editingCodeFor, setEditingCodeFor] = useState<number | null>(null);

  // Filter to only active entrants for the main list
  const activeEntrants = useMemo(
    () => entrants.filter((e) => e.bracketState === "active"),
    [entrants]
  );

  // Load entrants from backend
  const loadEntrants = useCallback(async () => {
    try {
      const result = await invoke<UnifiedEntrant[]>("get_unified_entrants");
      setEntrants(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to load entrants: ${msg}`);
    }
  }, []);

  // Sync entrants from Start.gg state
  const syncFromStartgg = useCallback(async () => {
    try {
      const count = await invoke<number>("sync_entrants_from_startgg");
      if (count > 0) {
        await loadEntrants();
        setEntrantsStatus(`Synced ${count} entrants from Start.gg`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Sync failed: ${msg}`);
    }
  }, [loadEntrants]);

  // Load setups sorted by seed
  const loadSetups = useCallback(async () => {
    try {
      const result = await invoke<SetupWithSeed[]>("get_setups_sorted_by_seed");
      setSetupsWithSeed(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to load setups: ${msg}`);
    }
  }, []);

  // Set slippi code for an entrant
  const setSlippiCode = useCallback(async (entrantId: number, code: string | null) => {
    try {
      await invoke("set_entrant_slippi_code", { entrantId, code });
      await loadEntrants();
      setEntrantsStatus(`Updated slippi code for entrant ${entrantId}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to set slippi code: ${msg}`);
    }
  }, [loadEntrants]);

  // Assign entrant to setup
  const assignToSetup = useCallback(async (entrantId: number, setupId: number | null) => {
    try {
      await invoke("assign_entrant_to_setup", { entrantId, setupId });
      await Promise.all([loadEntrants(), loadSetups()]);
      if (setupId) {
        setEntrantsStatus(`Assigned entrant to setup ${setupId}`);
      } else {
        setEntrantsStatus(`Unassigned entrant from setup`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to assign: ${msg}`);
    }
  }, [loadEntrants, loadSetups]);

  // Unassign entrant
  const unassignEntrant = useCallback(async (entrantId: number) => {
    try {
      await invoke("unassign_entrant", { entrantId });
      await Promise.all([loadEntrants(), loadSetups()]);
      setEntrantsStatus(`Unassigned entrant`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to unassign: ${msg}`);
    }
  }, [loadEntrants, loadSetups]);

  // Toggle auto-assignment
  const toggleAutoAssignment = useCallback(async (enabled: boolean) => {
    try {
      await invoke("toggle_auto_assignment", { enabled });
      setAutoAssignEnabled(enabled);
      setEntrantsStatus(enabled ? "Auto-assignment enabled" : "Auto-assignment disabled");
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Failed to toggle auto-assignment: ${msg}`);
    }
  }, []);

  // Manual run of auto-assignment
  const runAutoAssignment = useCallback(async () => {
    try {
      const assignments = await invoke<[number, number][]>("run_auto_assignment");
      await Promise.all([loadEntrants(), loadSetups()]);
      if (assignments.length > 0) {
        setEntrantsStatus(`Auto-assigned ${assignments.length} entrants`);
      } else {
        setEntrantsStatus("No entrants to auto-assign");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
      setEntrantsStatus(`Auto-assignment failed: ${msg}`);
    }
  }, [loadEntrants, loadSetups]);

  // Selection helpers
  const selectEntrant = useCallback((entrantId: number | null) => {
    setSelectedEntrantId(entrantId);
  }, []);

  const openCodeEditor = useCallback((entrantId: number) => {
    setEditingCodeFor(entrantId);
  }, []);

  const closeCodeEditor = useCallback(() => {
    setEditingCodeFor(null);
  }, []);

  // Get entrant by ID helper
  const getEntrantById = useCallback(
    (id: number) => entrants.find((e) => e.id === id),
    [entrants]
  );

  // Listen for entrants_updated events from backend
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<UnifiedEntrant[]>("entrants_updated", (event) => {
      setEntrants(event.payload);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        unlisten = null;
      });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // Load initial auto-assignment status
  useEffect(() => {
    invoke<boolean>("get_auto_assignment_status")
      .then((enabled) => setAutoAssignEnabled(enabled))
      .catch(() => {});
  }, []);

  // Escape key to clear selection
  useEffect(() => {
    if (selectedEntrantId === null && editingCodeFor === null) return;

    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (editingCodeFor !== null) {
          setEditingCodeFor(null);
        } else {
          setSelectedEntrantId(null);
        }
      }
    };

    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [selectedEntrantId, editingCodeFor]);

  return {
    entrants,
    activeEntrants,
    setupsWithSeed,
    autoAssignEnabled,
    entrantsStatus,
    selectedEntrantId,
    editingCodeFor,
    loadEntrants,
    loadSetups,
    syncFromStartgg,
    setSlippiCode,
    assignToSetup,
    unassignEntrant,
    toggleAutoAssignment,
    runAutoAssignment,
    selectEntrant,
    openCodeEditor,
    closeCodeEditor,
    getEntrantById,
  };
}
