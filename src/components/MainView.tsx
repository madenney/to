import type { DragEvent } from "react";
import type {
  AppConfig,
  Setup,
  SlippiStream,
  StartggSimEntrant,
  StartggSimState,
} from "../types/overlay";
import {
  stripSponsorTag,
  findExpectedOpponent,
  findSetForStream,
  opponentMatchesExpected,
  bestSeedForSet,
  isActiveSet,
} from "../tournamentUtils";

const MAX_SETUPS = 16;

type MainViewProps = {
  config: AppConfig;
  setups: Setup[];
  streams: SlippiStream[];
  setupStatus: string;
  topStatus: string;
  eventName: string;
  needsStartggLink: boolean;
  needsStartggToken: boolean;
  startggTokenError: boolean;
  startggLiveError: string;
  startggPollLoading: boolean;
  highlightSetupId: number | null;
  currentStartggState: StartggSimState | null;
  liveStreamIds: Set<string>;
  entrantLookup: Map<number, StartggSimEntrant>;
  streamEntrantLinks: Record<string, number>;
  draggedStreamId: string | null;
  streamSetupSelections: Record<string, number>;
  attendeeList: StartggSimEntrant[];
  attendeeStatusMap: Map<number, { state: string; label: string }>;
  attendeePlayingMap: Map<number, boolean>;
  attendeeBroadcastMap: Map<number, boolean>;
  slippiIsOpen: boolean;
  resolveStreamEntrantId: (stream: SlippiStream) => number | null;
  openSettings: (options?: { focusStartgg?: "link" | "token" }) => void;
  openBracketWindow: () => void;
  pollStartggCycle: () => Promise<void>;
  addSetup: () => Promise<void>;
  removeLastSetup: () => Promise<void>;
  openSetupDetails: (setupId: number) => void;
  clearSetup: (id: number) => Promise<void>;
  launchSetupStream: (setup: Setup) => Promise<void>;
  rebuildAutoStreamAssignments: () => Promise<void>;
  launchSlippi: () => Promise<void>;
  refreshSlippiThenScan: () => Promise<void>;
  handleStreamDragStart: (event: DragEvent, stream: SlippiStream) => void;
  handleStreamDragEnd: () => void;
  handleAttendeeDragOver: (event: DragEvent) => void;
  handleAttendeeDrop: (event: DragEvent, entrant: StartggSimEntrant) => void;
  handleSetupSelect: (stream: SlippiStream, value: string) => void;
  getStreamSetupId: (streamId: string) => number | null;
  unlinkStream: (streamId: string) => void;
};

export default function MainView({
  config,
  setups,
  streams,
  setupStatus,
  topStatus,
  eventName,
  needsStartggLink,
  needsStartggToken,
  startggTokenError,
  startggLiveError,
  startggPollLoading,
  highlightSetupId,
  currentStartggState,
  liveStreamIds,
  entrantLookup,
  streamEntrantLinks,
  draggedStreamId,
  streamSetupSelections,
  attendeeList,
  attendeeStatusMap,
  attendeePlayingMap,
  attendeeBroadcastMap,
  slippiIsOpen,
  resolveStreamEntrantId,
  openSettings,
  openBracketWindow,
  pollStartggCycle,
  addSetup,
  removeLastSetup,
  openSetupDetails,
  clearSetup,
  launchSetupStream,
  rebuildAutoStreamAssignments,
  launchSlippi,
  refreshSlippiThenScan,
  handleStreamDragStart,
  handleStreamDragEnd,
  handleAttendeeDragOver,
  handleAttendeeDrop,
  handleSetupSelect,
  getStreamSetupId,
  unlinkStream,
}: MainViewProps) {
  return (
    <main className="app">
      <div className="top-bar">
        <div className="top-bar-left">
          {eventName && <div className="chip event-chip">{eventName}</div>}
          {config.testMode && <div className="chip test-mode-chip">Test mode</div>}
        </div>
        {config.testMode && (
          <button className="ghost-btn small" onClick={openBracketWindow}>
            Spoof bracket
          </button>
        )}
        <button
          className="ghost-btn small poll-btn"
          onClick={pollStartggCycle}
          disabled={startggPollLoading}
        >
          {startggPollLoading && <span className="btn-spinner" aria-hidden="true" />}
          Poll Start.gg
        </button>
        <button className="icon-button" onClick={() => openSettings()} aria-label="Open settings">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 8.6a3.4 3.4 0 1 0 0 6.8 3.4 3.4 0 0 0 0-6.8zm9 3.4c0-.5-.04-1-.12-1.48l2.12-1.65-2-3.46-2.54 1a8.7 8.7 0 0 0-2.56-1.48l-.38-2.7h-4l-.38 2.7c-.9.28-1.76.76-2.56 1.48l-2.54-1-2 3.46 2.12 1.65c-.08.48-.12.98-.12 1.48s.04 1 .12 1.48L1.94 14.99l2 3.46 2.54-1a8.7 8.7 0 0 0 2.56 1.48l.38 2.7h4l.38-2.7c.9-.28 1.76-.76 2.56-1.48l2.54 1 2-3.46-2.12-1.65c.08-.48.12-.98.12-1.48zM12 17.2a5.2 5.2 0 1 1 0-10.4 5.2 5.2 0 0 1 0 10.4z" />
          </svg>
        </button>
      </div>
      {needsStartggLink && (
        <button
          type="button"
          className="status-line warning warning-action"
          onClick={() => openSettings({ focusStartgg: "link" })}
        >
          Live mode needs a Start.gg event link. Click to add one in Settings.
        </button>
      )}
      {!needsStartggLink && (needsStartggToken || startggTokenError) && (
        <button
          type="button"
          className="status-line warning warning-action"
          onClick={() => openSettings({ focusStartgg: "token" })}
        >
          {needsStartggToken
            ? "Live mode needs a Start.gg API token. Click to add one in Settings."
            : `Start.gg error: ${startggLiveError} Click to review settings.`}
        </button>
      )}
      {topStatus && <div className="status-line">{topStatus}</div>}

      <section className="panel setups-panel">
        <div className="section-header">
          <div>
            <p className="eyebrow">Setups</p>
          </div>
          <div className="action-row">
            <button
              className="ghost-btn small"
              onClick={rebuildAutoStreamAssignments}
              disabled={!config.autoStream}
              aria-label="Rebuild auto stream assignments"
            >
              Rebuild
            </button>
            <button
              className="ghost-btn small setup-adjust"
              onClick={addSetup}
              disabled={setups.length >= MAX_SETUPS}
              aria-label="Add setup"
            >
              +
            </button>
            <button
              className="ghost-btn small setup-adjust"
              onClick={removeLastSetup}
              disabled={setups.length === 0}
              aria-label="Remove last setup"
            >
              -
            </button>
          </div>
        </div>
        {setupStatus && <div className="status-line">{setupStatus}</div>}
        <div className="setup-grid">
          {setups.length === 0 ? (
            <div className="todo-box">No setups yet. Click + to get started.</div>
          ) : (
            setups.map((s) => {
              const assigned = s.assignedStream;
              const p1Name =
                stripSponsorTag(assigned?.p1Tag) || assigned?.p1Code || "Waiting";
              const p1Sub = assigned?.p1Tag ? (assigned?.p1Code ?? "N/A") : "N/A";
              const hasAssignedP2 = Boolean(assigned?.p2Tag || assigned?.p2Code);
              const expectedOpponent = findExpectedOpponent(
                currentStartggState,
                assigned,
                resolveStreamEntrantId,
              );
              const displayExpected = !hasAssignedP2 ? expectedOpponent : null;
              const isOffline = Boolean(assigned && !liveStreamIds.has(assigned.id));
              const startggSet = assigned
                ? findSetForStream(currentStartggState, assigned, resolveStreamEntrantId)
                : null;
              const isMatchActive = Boolean(startggSet && startggSet.state === "inProgress");
              const isNonTourney =
                Boolean(assigned?.isPlaying) &&
                !isOffline &&
                Boolean(expectedOpponent) &&
                !opponentMatchesExpected(expectedOpponent, assigned?.p2Tag, assigned?.p2Code);
              const p2Name =
                stripSponsorTag(assigned?.p2Tag) ||
                assigned?.p2Code ||
                stripSponsorTag(displayExpected?.tag) ||
                displayExpected?.code ||
                "Waiting";
              const p2Sub = assigned?.p2Tag
                ? (assigned?.p2Code ?? "N/A")
                : displayExpected
                  ? (displayExpected.code ?? "Expected")
                  : "N/A";
              const setupStatusText = assigned
                ? isOffline
                  ? "Offline"
                  : isNonTourney
                    ? "Non tourney"
                    : assigned.isPlaying
                      ? "Playing"
                      : "Waiting for game..."
                : "Unassigned";
              const isHighlighted = highlightSetupId === s.id;
              return (
                <article
                  key={s.id}
                  className={`setup-card ${isHighlighted ? "highlight" : ""}`}
                  role="button"
                  tabIndex={0}
                  onClick={() => openSetupDetails(s.id)}
                  onKeyDown={(event) => {
                    if (event.currentTarget !== event.target) {
                      return;
                    }
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      openSetupDetails(s.id);
                    }
                  }}
                  aria-label={`Open details for ${s.name}`}
                >
                  <div className={`setup-main ${assigned?.isPlaying && !isOffline && !isNonTourney ? "playing" : isMatchActive ? "match-active" : ""}`}>
                    <div className="setup-header">
                      <div className="setup-name">
                        {s.name}
                        {isMatchActive && (
                          <span className="setup-match-indicator" title="Match active" />
                        )}
                      </div>
                      <div className="setup-actions">
                        {assigned && (
                          <>
                            <button
                              className="ghost-btn small"
                              type="button"
                              disabled={isOffline}
                              onClick={(event) => {
                                event.stopPropagation();
                                launchSetupStream(s);
                              }}
                            >
                              Watch
                            </button>
                            <button
                              className="ghost-btn small"
                              type="button"
                              onClick={(event) => {
                                event.stopPropagation();
                                clearSetup(s.id);
                              }}
                            >
                              Clear
                            </button>
                          </>
                        )}
                      </div>
                    </div>
                    <div className="setup-meta">
                      <div className="muted">Status: {setupStatusText}</div>
                    </div>
                  </div>
                  <div className="setup-seats">
                    <div className="setup-seat">
                      <div className="seat-label">P1</div>
                      <div className="seat-name">{p1Name}</div>
                      <div className="seat-code">{p1Sub}</div>
                    </div>
                    <div className="setup-seat right">
                      <div className="seat-label">P2</div>
                      <div className="seat-name">{p2Name}</div>
                      <div className="seat-code">{p2Sub}</div>
                    </div>
                  </div>
                </article>
              );
            })
          )}
        </div>
      </section>

      <section className="panel">
        <div className="section-header">
          <div>
            <p className="eyebrow">Broadcasting</p>
          </div>
          <div className="action-row">
            <button
              className="ghost-btn"
              onClick={launchSlippi}
              title={slippiIsOpen ? "Relaunch Slippi Launcher" : "Launch Slippi Launcher"}
            >
              {slippiIsOpen ? "Relaunch Slippi" : "Launch Slippi"}
            </button>
            <button className="ghost-btn" onClick={refreshSlippiThenScan}>
              Refresh
            </button>
          </div>
        </div>
        <div className="broadcast-grid">
          {streams.length === 0 ? (
            <div className="broadcast-empty">No active broadcasts found.</div>
          ) : (
            streams.map((s) => {
              const isPlaying = s.isPlaying === true;
              const linkedEntrantId = streamEntrantLinks[s.id];
              const linkedEntrant =
                linkedEntrantId && Number.isFinite(linkedEntrantId)
                  ? entrantLookup.get(linkedEntrantId)
                  : null;
              const assignedSetupId = getStreamSetupId(s.id);
              return (
                <article
                  key={s.id}
                  className={`broadcast-card ${isPlaying ? "playing" : ""} ${draggedStreamId === s.id ? "dragging" : ""}`}
                  draggable
                  onDragStart={(event) => handleStreamDragStart(event, s)}
                  onDragEnd={handleStreamDragEnd}
                >
                  <div className="broadcast-card-header">
                    <div className="broadcast-player-name">
                      {stripSponsorTag(s.p1Tag) || "Unknown"}
                    </div>
                    {isPlaying && <span className="broadcast-live-badge">Live</span>}
                  </div>
                  <div className="broadcast-card-code">{s.p1Code ?? "N/A"}</div>
                  {linkedEntrant && (
                    <div className="broadcast-linked">
                      <span>→ {stripSponsorTag(linkedEntrant.name)}</span>
                      <button
                        className="broadcast-unlink"
                        onClick={() => unlinkStream(s.id)}
                        type="button"
                      >
                        ×
                      </button>
                    </div>
                  )}
                  <div className="broadcast-card-footer">
                    <select
                      className="broadcast-setup-select"
                      value={assignedSetupId ?? ""}
                      onChange={(e) => handleSetupSelect(s, e.target.value)}
                    >
                      <option value="">Assign to setup…</option>
                      {setups.map((setup) => (
                        <option key={setup.id} value={setup.id}>
                          {setup.name}
                        </option>
                      ))}
                    </select>
                  </div>
                </article>
              );
            })
          )}
        </div>
      </section>

      <section className="panel">
        <div className="section-header">
          <div>
            <p className="eyebrow">Attendees</p>
          </div>
          <div className="muted tiny">{attendeeList.length}</div>
        </div>
        <div className="broadcast-list">
          {attendeeList.length === 0 ? (
            <div className="muted tiny">No attendees loaded.</div>
          ) : (
            attendeeList.map((entrant) => {
              const status = attendeeStatusMap.get(entrant.id) ?? {
                state: "waiting",
                label: "Waiting",
              };
              const isPlaying = attendeePlayingMap.get(entrant.id) === true;
              const isBroadcasting = attendeeBroadcastMap.get(entrant.id) === true;
              return (
                <div
                  key={entrant.id}
                  className={`broadcast-item attendee-item ${draggedStreamId ? "drop-target" : ""}`}
                  onDragOver={handleAttendeeDragOver}
                  onDrop={(event) => handleAttendeeDrop(event, entrant)}
                >
                  <span className="broadcast-info">
                    <span className="broadcast-name">
                      {stripSponsorTag(entrant.name) || "Unknown"}
                    </span>
                    <span className="muted tiny code">{entrant.slippiCode || "No code"}</span>
                  </span>
                  <div className="attendee-badges">
                    <span className="attendee-status" data-state={status.state}>
                      {status.label}
                    </span>
                    {isBroadcasting && (
                      <span className="attendee-status" data-state="broadcasting">
                        Broadcasting
                      </span>
                    )}
                    {isPlaying && (
                      <span className="attendee-status" data-state="playing">
                        Playing
                      </span>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </section>
    </main>
  );
}
