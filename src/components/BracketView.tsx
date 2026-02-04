import { type CSSProperties, type DragEvent, type MouseEvent } from "react";
import type {
  AppConfig,
  StartggSimEntrant,
  StartggSimSet,
  StartggSimSlot,
  StartggSimState,
  ReplayStreamUpdate,
} from "../types/overlay";
import { stripSponsorTag, columnLabel, formatSetState } from "../tournamentUtils";

const BRACKET_SET_HEIGHT = 88;

type BracketRounds = {
  winnersRounds: [number, StartggSimState["sets"]][];
  losersRounds: [number, StartggSimState["sets"]][];
  winnersBase: number;
  losersBase: number;
};

type BracketViewProps = {
  config: AppConfig;
  bracketState: StartggSimState | null;
  bracketStatus: string;
  bracketRounds: BracketRounds | null;
  bracketZoom: number;
  isBracketPanning: boolean;
  isDraggingReplays: boolean;
  bracketDropTarget: number | null;
  recentDropSetId: number | null;
  replaySet: Set<number>;
  replayStreamUpdate: ReplayStreamUpdate | null;
  replayStreamStartedAt: number | null;
  broadcastEntrants: StartggSimEntrant[];
  broadcastSelections: Record<number, boolean>;
  broadcastActiveCount: number;
  attendeeList: StartggSimEntrant[];
  attendeeStatusMap: Map<number, { state: string; label: string }>;
  attendeePlayingMap: Map<number, boolean>;
  attendeeBroadcastMap: Map<number, boolean>;
  bracketScrollRef: React.RefObject<HTMLDivElement | null>;
  resolveSlotLabel: (slot: StartggSimSlot) => string;
  openBracketSettings: () => void;
  openBracketSetDetails: (setId: number) => void;
  openEventLink: (url: string) => void;
  completeBracket: () => Promise<void>;
  cancelReplayStream: () => Promise<void>;
  streamBracketReplay: (setId: number) => Promise<void>;
  toggleBroadcast: (entrantId: number) => void;
  handleBracketDragEnter: (event: DragEvent<HTMLDivElement>) => void;
  handleBracketDragLeave: (event: DragEvent<HTMLDivElement>) => void;
  handleSetDragOver: (event: DragEvent<HTMLDivElement>, setId: number) => void;
  handleSetDragLeave: (setId: number) => void;
  handleSetDrop: (event: DragEvent<HTMLDivElement>, setId: number) => void;
  handleBracketPanStart: (event: MouseEvent<HTMLDivElement>) => void;
  hasFileDrag: (event: DragEvent<HTMLElement>) => boolean;
  resetBracketDragState: () => void;
};

function columnHeight(baseCount: number): string | undefined {
  if (!baseCount) {
    return undefined;
  }
  return `${baseCount * BRACKET_SET_HEIGHT}px`;
}

export default function BracketView({
  config,
  bracketState,
  bracketStatus,
  bracketRounds,
  bracketZoom,
  isBracketPanning,
  isDraggingReplays,
  bracketDropTarget,
  recentDropSetId,
  replaySet,
  replayStreamUpdate,
  replayStreamStartedAt,
  broadcastEntrants,
  broadcastSelections,
  broadcastActiveCount,
  attendeeList,
  attendeeStatusMap,
  attendeePlayingMap,
  attendeeBroadcastMap,
  bracketScrollRef,
  resolveSlotLabel,
  openBracketSettings,
  openBracketSetDetails,
  openEventLink,
  completeBracket,
  cancelReplayStream,
  streamBracketReplay,
  toggleBroadcast,
  handleBracketDragEnter,
  handleBracketDragLeave,
  handleSetDragOver,
  handleSetDragLeave,
  handleSetDrop,
  handleBracketPanStart,
  hasFileDrag,
  resetBracketDragState,
}: BracketViewProps) {
  const replayStreamSet = bracketState?.sets.find(
    (set) => set.id === replayStreamUpdate?.setId,
  ) ?? null;

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
    ? `${resolveSlotLabel(replayStreamSet.slots[0])} vs ${resolveSlotLabel(replayStreamSet.slots[1])}`
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
  const canCancelReplay =
    replayStreamUpdate?.type === "start" || replayStreamUpdate?.type === "progress";

  function renderBracketSet(set: StartggSimSet) {
    const isByeSet = set.slots.some((slot) => slot.sourceType === "empty");
    const isDqSet = set.slots.some((slot) => slot.result === "dq");
    const hasReplay = replaySet.has(set.id);
    const isDropTarget = bracketDropTarget === set.id;
    const isDropReady = isDraggingReplays;
    const isDropSuccess = recentDropSetId === set.id;
    const stateLabel = formatSetState(set.state);
    const stateData = set.state;
    const setClassName = [
      "bracket-set",
      isByeSet ? "bye" : "",
      isDqSet ? "dq" : "",
      hasReplay ? "has-replay" : "",
      isDropReady ? "drop-ready" : "",
      isDropTarget ? "drop-target" : "",
      isDropSuccess ? "drop-success" : "",
    ]
      .filter(Boolean)
      .join(" ");

    return (
      <div
        key={set.id}
        className={setClassName}
        data-set-id={set.id}
        role="button"
        tabIndex={0}
        onDragOver={(event) => handleSetDragOver(event, set.id)}
        onDragLeave={() => handleSetDragLeave(set.id)}
        onDrop={(event) => handleSetDrop(event, set.id)}
        onClick={(event) => {
          if (isDraggingReplays) {
            return;
          }
          if ((event.target as HTMLElement).closest("button")) {
            return;
          }
          openBracketSetDetails(set.id);
        }}
        onKeyDown={(event) => {
          if (event.currentTarget !== event.target) {
            return;
          }
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            openBracketSetDetails(set.id);
          }
        }}
        aria-label={`Open set ${set.id} details`}
      >
        <div className="bracket-set-header">
          <div className="bracket-set-meta">
            <span className="state-pill" data-state={stateData}>
              {stateLabel}
            </span>
            {hasReplay && (
              <button
                type="button"
                className="replay-pill"
                onClick={(event) => {
                  event.stopPropagation();
                  streamBracketReplay(set.id);
                }}
              >
                Replay All
              </button>
            )}
          </div>
        </div>
        {set.slots.map((slot, idx) => {
          const slotLabel = resolveSlotLabel(slot);
          const scoreLabel = slot.result === "dq" ? "DQ" : slot.score;
          const showScore = scoreLabel !== null && scoreLabel !== undefined;
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

  return (
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
          <div className="action-row">
            <div className="streaming-badge" data-status={replayStatus.tone}>
              {replayStatus.label}
            </div>
            <button
              className="ghost-btn small"
              type="button"
              onClick={cancelReplayStream}
              disabled={!canCancelReplay}
            >
              Cancel
            </button>
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
          <div
            className="bracket-stage"
            onDragEnter={handleBracketDragEnter}
            onDragLeave={handleBracketDragLeave}
            onDrop={(event) => {
              if (hasFileDrag(event)) {
                event.preventDefault();
              }
              resetBracketDragState();
            }}
            onDragOver={(event) => {
              if (!hasFileDrag(event)) {
                return;
              }
              event.preventDefault();
            }}
          >
            <aside className="broadcast-panel">
              <div className="broadcast-header">
                <p className="eyebrow">Broadcast</p>
                <div className="muted tiny">
                  {broadcastActiveCount} of {broadcastEntrants.length} on
                </div>
              </div>
              <div className="broadcast-list">
                {broadcastEntrants.length === 0 ? (
                  <div className="muted tiny">No entrants loaded.</div>
                ) : (
                  broadcastEntrants.map((entrant) => {
                    const isOn = Boolean(broadcastSelections[entrant.id]);
                    const code = entrant.slippiCode?.trim();
                    return (
                      <label
                        key={entrant.id}
                        className={`broadcast-item ${isOn ? "on" : "off"}`}
                      >
                        <input
                          type="checkbox"
                          className="broadcast-switch"
                          checked={isOn}
                          onChange={() => toggleBroadcast(entrant.id)}
                        />
                        <span className="broadcast-info">
                          <span className="broadcast-name">
                            {stripSponsorTag(entrant.name) || "Unknown"}
                          </span>
                          <span className="muted tiny code">{code || "No code"}</span>
                        </span>
                      </label>
                    );
                  })
                )}
              </div>
            </aside>
            <aside className="broadcast-panel attendee-panel">
              <div className="broadcast-header">
                <p className="eyebrow">Attendees</p>
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
                      <div key={entrant.id} className="broadcast-item attendee-item">
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
            </aside>
            <div
              className={`bracket-zoom-frame ${isBracketPanning ? "is-panning" : ""}`}
              ref={bracketScrollRef}
              onMouseDown={handleBracketPanStart}
              onAuxClick={(event) => {
                if (event.button === 1) {
                  event.preventDefault();
                }
              }}
            >
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
          </div>
        )}
      </section>
    </main>
  );
}
