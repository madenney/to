import { useEffect, useCallback, useMemo } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useConfig } from "./hooks/useConfig";
import { useSetups } from "./hooks/useSetups";
import { useStreams } from "./hooks/useStreams";
import { useBracket } from "./hooks/useBracket";
import { useAttendees } from "./hooks/useAttendees";
import MainView from "./components/MainView";
import BracketView from "./components/BracketView";
import SettingsModal from "./components/SettingsModal";
import SetupDetailsModal from "./components/SetupDetailsModal";
import BracketSettingsModal from "./components/BracketSettingsModal";
import BracketSetDetailsModal from "./components/BracketSetDetailsModal";
import {
  findExpectedOpponent,
  findSetForStream,
  bestSeedForSet,
  isActiveSet,
  resolveSlotLabel as resolveSlotLabelUtil,
} from "./tournamentUtils";
import type { StartggSimSlot } from "./types/overlay";

export default function App() {
  const isBracketView = useMemo(() => {
    const params = new URLSearchParams(window.location.search);
    return params.get("view") === "bracket";
  }, []);

  // ── Config hook ─────────────────────────────────────────────────────────
  const configHook = useConfig(isBracketView, {
    resetBracketState: undefined as any, // wired below via effect
    setTopStatus: undefined as any,
    setBracketStatus: undefined as any,
  });

  // ── Setups hook ─────────────────────────────────────────────────────────
  const setupsHook = useSetups(isBracketView);

  // ── Streams hook ────────────────────────────────────────────────────────
  const streamsHook = useStreams({
    isBracketView,
    config: configHook.config,
    setups: setupsHook.setups,
    setSetups: setupsHook.setSetups,
    currentStartggState: configHook.currentStartggState,
    autoManagedSetupIds: setupsHook.autoManagedSetupIds,
    streamSetupSelections: setupsHook.streamSetupSelections,
    setStreamSetupSelections: setupsHook.setStreamSetupSelections,
    clearSetupAssignment: setupsHook.clearSetupAssignment,
    setEphemeralSetupStatus: setupsHook.setEphemeralSetupStatus,
    setPersistentSetupStatus: setupsHook.setPersistentSetupStatus,
    refreshTestStartggState: configHook.refreshTestStartggState,
  });

  // ── Bracket hook ────────────────────────────────────────────────────────
  const bracketHook = useBracket({
    isBracketView,
    config: configHook.config,
    selectedBracketPath: configHook.selectedBracketPath,
    setTestStartggState: configHook.setTestStartggState,
    loadBracketConfigs: configHook.loadBracketConfigs,
  });

  // ── Attendees hook ──────────────────────────────────────────────────────
  const attendeesHook = useAttendees({
    currentStartggState: configHook.currentStartggState,
    streams: streamsHook.streams,
    linkedStreamByEntrantId: streamsHook.linkedStreamByEntrantId,
  });

  // ── Cross-hook wiring ───────────────────────────────────────────────────

  // Wire config deps that require bracket/stream hooks (circular ref workaround)
  useEffect(() => {
    // This effect handles the initial load for the main view
    if (isBracketView) return;
    setupsHook.loadSetups();
    streamsHook.refreshStreams();
    configHook.loadConfig();
  }, [isBracketView]);

  // Auto stream assignments
  useEffect(() => {
    if (isBracketView || !configHook.config.autoStream) return;
    streamsHook.applyAutoStreamAssignments();
  }, [
    isBracketView,
    configHook.config.autoStream,
    configHook.currentStartggState,
    streamsHook.streams,
    setupsHook.setups,
  ]);

  // Fit seat names on resize and data changes
  const fitSeatNames = useCallback(() => {
    if (isBracketView) return;
    const elements = document.querySelectorAll<HTMLElement>(".seat-name");
    elements.forEach((el) => {
      if (!el.clientWidth) return;
      el.style.fontSize = "";
      let size = Number.parseFloat(window.getComputedStyle(el).fontSize || "0");
      if (!Number.isFinite(size) || size <= 0) size = 15;
      const min = 12;
      let guard = 20;
      while (el.scrollWidth > el.clientWidth && size > min && guard-- > 0) {
        size -= 1;
        el.style.fontSize = `${size}px`;
      }
    });
  }, [isBracketView, configHook.config.testMode]);

  useEffect(() => {
    if (isBracketView) return;
    const handle = () => window.requestAnimationFrame(() => fitSeatNames());
    window.addEventListener("resize", handle);
    handle();
    return () => window.removeEventListener("resize", handle);
  }, [isBracketView, fitSeatNames]);

  useEffect(() => {
    if (isBracketView) return;
    window.requestAnimationFrame(() => fitSeatNames());
  }, [isBracketView, setupsHook.setups, configHook.currentStartggState, fitSeatNames]);

  // Spoof replay progress listener for main view (refreshes streams)
  useEffect(() => {
    if (isBracketView || !configHook.config.testMode) return;
    let unlisten: UnlistenFn | null = null;
    listen("spoof-replay-progress", (event) => {
      const payload = event.payload as Record<string, unknown> | null;
      const shouldRefresh =
        payload?.type === "start" || payload?.type === "complete" || payload?.type === "error";
      if (shouldRefresh) {
        streamsHook.refreshStreamsRef.current?.();
      }
    }).then((fn) => { unlisten = fn; }).catch(() => { unlisten = null; });
    return () => { if (unlisten) unlisten(); };
  }, [isBracketView, configHook.config.testMode]);

  // ── Derived values ──────────────────────────────────────────────────────

  const eventName = (configHook.currentStartggState?.event?.name ?? "").trim();

  const highlightSetupId = useMemo(() => {
    if (!configHook.config.autoStream || !configHook.currentStartggState) return null;
    let best: { id: number; seed: number } | null = null;
    for (const setup of setupsHook.setups) {
      const assigned = setup.assignedStream;
      if (!assigned) continue;
      const set = findSetForStream(
        configHook.currentStartggState,
        assigned,
        streamsHook.resolveStreamEntrantId,
      );
      if (!set || !isActiveSet(set)) continue;
      const seed = bestSeedForSet(set);
      if (!best || seed < best.seed || (seed === best.seed && setup.id < best.id)) {
        best = { id: setup.id, seed };
      }
    }
    return best?.id ?? null;
  }, [configHook.config.autoStream, configHook.currentStartggState, setupsHook.setups]);

  const setupDetailsExpectedOpponent = useMemo(() => {
    const details = setupsHook.setupDetails;
    if (!details) return null;
    const stream = details.assignedStream;
    if (!stream) return null;
    if (stream.p2Tag || stream.p2Code) return null;
    return findExpectedOpponent(
      configHook.currentStartggState,
      stream,
      streamsHook.resolveStreamEntrantId,
    );
  }, [setupsHook.setupDetails, configHook.currentStartggState]);

  // resolveSlotLabel needs bracketSetsById from bracket hook
  const resolveSlotLabel = useCallback(
    (slot: StartggSimSlot) => resolveSlotLabelUtil(slot, bracketHook.bracketSetsById),
    [bracketHook.bracketSetsById],
  );

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <>
      {isBracketView ? (
        <BracketView
          config={configHook.config}
          bracketState={bracketHook.bracketState}
          bracketStatus={bracketHook.bracketStatus}
          bracketRounds={bracketHook.bracketRounds}
          bracketZoom={bracketHook.bracketZoom}
          isBracketPanning={bracketHook.isBracketPanning}
          isDraggingReplays={bracketHook.isDraggingReplays}
          bracketDropTarget={bracketHook.bracketDropTarget}
          recentDropSetId={bracketHook.recentDropSetId}
          replaySet={bracketHook.replaySet}
          replayStreamUpdate={bracketHook.replayStreamUpdate}
          replayStreamStartedAt={bracketHook.replayStreamStartedAt}
          broadcastEntrants={bracketHook.broadcastEntrants}
          broadcastSelections={bracketHook.broadcastSelections}
          broadcastActiveCount={bracketHook.broadcastActiveCount}
          attendeeList={attendeesHook.attendeeList}
          attendeeStatusMap={attendeesHook.attendeeStatusMap}
          attendeePlayingMap={attendeesHook.attendeePlayingMap}
          attendeeBroadcastMap={attendeesHook.attendeeBroadcastMap}
          bracketScrollRef={bracketHook.bracketScrollRef}
          resolveSlotLabel={resolveSlotLabel}
          openBracketSettings={bracketHook.openBracketSettings}
          openBracketSetDetails={bracketHook.openBracketSetDetails}
          openEventLink={bracketHook.openEventLink}
          completeBracket={bracketHook.completeBracket}
          cancelReplayStream={bracketHook.cancelReplayStream}
          streamBracketReplay={bracketHook.streamBracketReplay}
          toggleBroadcast={bracketHook.toggleBroadcast}
          handleBracketDragEnter={bracketHook.handleBracketDragEnter}
          handleBracketDragLeave={bracketHook.handleBracketDragLeave}
          handleSetDragOver={bracketHook.handleSetDragOver}
          handleSetDragLeave={bracketHook.handleSetDragLeave}
          handleSetDrop={bracketHook.handleSetDrop}
          handleBracketPanStart={bracketHook.handleBracketPanStart}
          hasFileDrag={bracketHook.hasFileDrag}
          resetBracketDragState={bracketHook.resetBracketDragState}
        />
      ) : (
        <MainView
          config={configHook.config}
          setups={setupsHook.setups}
          streams={streamsHook.streams}
          setupStatus={setupsHook.setupStatus}
          topStatus={streamsHook.topStatus}
          eventName={eventName}
          needsStartggLink={configHook.needsStartggLink}
          needsStartggToken={configHook.needsStartggToken}
          startggTokenError={configHook.startggTokenError}
          startggLiveError={configHook.startggLiveError}
          startggPollLoading={configHook.startggPollLoading}
          highlightSetupId={highlightSetupId}
          currentStartggState={configHook.currentStartggState}
          liveStreamIds={streamsHook.liveStreamIds}
          entrantLookup={streamsHook.entrantLookup}
          streamEntrantLinks={streamsHook.streamEntrantLinks}
          draggedStreamId={streamsHook.draggedStreamId}
          streamSetupSelections={setupsHook.streamSetupSelections}
          attendeeList={attendeesHook.attendeeList}
          attendeeStatusMap={attendeesHook.attendeeStatusMap}
          attendeePlayingMap={attendeesHook.attendeePlayingMap}
          attendeeBroadcastMap={attendeesHook.attendeeBroadcastMap}
          slippiIsOpen={streamsHook.slippiIsOpen}
          resolveStreamEntrantId={streamsHook.resolveStreamEntrantId}
          openSettings={configHook.openSettings}
          openBracketWindow={streamsHook.openBracketWindow}
          pollStartggCycle={configHook.pollStartggCycle}
          addSetup={setupsHook.addSetup}
          removeLastSetup={setupsHook.removeLastSetup}
          openSetupDetails={setupsHook.openSetupDetails}
          clearSetup={setupsHook.clearSetup}
          launchSetupStream={streamsHook.launchSetupStream}
          rebuildAutoStreamAssignments={streamsHook.rebuildAutoStreamAssignments}
          launchSlippi={streamsHook.launchSlippi}
          refreshSlippiThenScan={streamsHook.refreshSlippiThenScan}
          handleStreamDragStart={streamsHook.handleStreamDragStart}
          handleStreamDragEnd={streamsHook.handleStreamDragEnd}
          handleAttendeeDragOver={streamsHook.handleAttendeeDragOver}
          handleAttendeeDrop={streamsHook.handleAttendeeDrop}
          handleSetupSelect={streamsHook.handleSetupSelect}
          getStreamSetupId={streamsHook.getStreamSetupId}
          unlinkStream={streamsHook.unlinkStream}
        />
      )}

      {isBracketView && bracketHook.bracketSettingsOpen && (
        <BracketSettingsModal
          config={configHook.config}
          bracketZoom={bracketHook.bracketZoom}
          setBracketZoom={bracketHook.setBracketZoom}
          setAutoCompleteBracket={configHook.setAutoCompleteBracket}
          resetBracketState={bracketHook.resetBracketState}
          refreshBracketState={bracketHook.refreshBracketState}
          closeBracketSettings={bracketHook.closeBracketSettings}
        />
      )}

      {isBracketView && bracketHook.bracketSetDetails && (
        <BracketSetDetailsModal
          bracketSetDetails={bracketHook.bracketSetDetails}
          bracketSetDetailsJson={bracketHook.bracketSetDetailsJson}
          bracketSetReplayPaths={bracketHook.bracketSetReplayPaths}
          bracketSetDetailsStatus={bracketHook.bracketSetDetailsStatus}
          bracketSetActionStatus={bracketHook.bracketSetActionStatus}
          bracketSetIsPending={bracketHook.bracketSetIsPending}
          bracketSetIsCompleted={bracketHook.bracketSetIsCompleted}
          hasReplay={bracketHook.replaySet.has(bracketHook.bracketSetDetails.id)}
          resolveSlotLabel={resolveSlotLabel}
          startMatchForSet={bracketHook.startMatchForSet}
          stepBracketSet={bracketHook.stepBracketSet}
          finalizeSetFromReference={bracketHook.finalizeSetFromReference}
          resetSet={bracketHook.resetSet}
          streamBracketReplayGame={bracketHook.streamBracketReplayGame}
          closeBracketSetDetails={bracketHook.closeBracketSetDetails}
        />
      )}

      {!isBracketView && configHook.settingsOpen && (
        <SettingsModal
          config={configHook.config}
          configStatus={configHook.configStatus}
          startggStatus={configHook.startggStatus}
          bracketConfigs={configHook.bracketConfigs}
          selectedBracketPath={configHook.selectedBracketPath}
          startggLinkInputRef={configHook.startggLinkInputRef}
          startggTokenInputRef={configHook.startggTokenInputRef}
          updateConfig={configHook.updateConfig}
          saveConfig={configHook.saveConfig}
          toggleTestMode={configHook.toggleTestMode}
          browsePath={configHook.browsePath}
          handleBracketSelect={configHook.handleBracketSelect}
          openBracketWindow={streamsHook.openBracketWindow}
          spoofLiveGames={streamsHook.spoofLiveGames}
          closeSettings={configHook.closeSettings}
        />
      )}

      {!isBracketView && setupsHook.setupDetails && (
        <SetupDetailsModal
          setupDetails={setupsHook.setupDetails}
          setupDetailsJson={setupsHook.setupDetailsJson}
          setupOverlayUrl={setupsHook.setupOverlayUrl}
          overlayCopyStatus={setupsHook.overlayCopyStatus}
          setupDetailsExpectedOpponent={setupDetailsExpectedOpponent}
          resolveSlotLabel={resolveSlotLabel}
          copyOverlayUrl={setupsHook.copyOverlayUrl}
          closeSetupDetails={setupsHook.closeSetupDetails}
        />
      )}
    </>
  );
}
