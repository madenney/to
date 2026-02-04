import type { StartggSimSet, StartggSimSlot } from "../types/overlay";

type BracketSetDetailsModalProps = {
  bracketSetDetails: StartggSimSet;
  bracketSetDetailsJson: string;
  bracketSetReplayPaths: string[];
  bracketSetDetailsStatus: string;
  bracketSetActionStatus: string;
  bracketSetIsPending: boolean;
  bracketSetIsCompleted: boolean;
  hasReplay: boolean;
  resolveSlotLabel: (slot: StartggSimSlot) => string;
  startMatchForSet: (setId: number) => Promise<void>;
  stepBracketSet: (setId: number) => Promise<void>;
  finalizeSetFromReference: (setId: number) => Promise<void>;
  resetSet: (setId: number) => Promise<void>;
  streamBracketReplayGame: (
    setId: number,
    replayPath: string,
    replayIndex: number,
    replayTotal: number,
  ) => void;
  closeBracketSetDetails: () => void;
};

export default function BracketSetDetailsModal({
  bracketSetDetails,
  bracketSetDetailsJson,
  bracketSetReplayPaths,
  bracketSetDetailsStatus,
  bracketSetActionStatus,
  bracketSetIsPending,
  bracketSetIsCompleted,
  hasReplay,
  resolveSlotLabel,
  startMatchForSet,
  stepBracketSet,
  finalizeSetFromReference,
  resetSet,
  streamBracketReplayGame,
  closeBracketSetDetails,
}: BracketSetDetailsModalProps) {
  return (
    <div className="modal-backdrop" onClick={closeBracketSetDetails}>
      <div
        className="modal bracket-set-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={`Set ${bracketSetDetails.id} details`}
      >
        <div className="modal-header">
          <div>
            <p className="eyebrow">Bracket Set</p>
            <div className="bracket-detail-title">
              {bracketSetDetails.roundLabel}
            </div>
          </div>
          <button className="icon-button" onClick={closeBracketSetDetails} aria-label="Close set details">
            x
          </button>
        </div>
        <div className="bracket-detail-grid">
          <div className="bracket-detail-card">
            <div className="label">Set Info</div>
            <div className="bracket-detail-row">
              <span className="bracket-detail-key">ID</span>
              <span className="bracket-detail-value">#{bracketSetDetails.id}</span>
            </div>
            <div className="bracket-detail-row">
              <span className="bracket-detail-key">Phase</span>
              <span className="bracket-detail-value">{bracketSetDetails.phaseName}</span>
            </div>
            <div className="bracket-detail-row">
              <span className="bracket-detail-key">State</span>
              <span className="bracket-detail-value">{bracketSetDetails.state}</span>
            </div>
            <div className="bracket-detail-row">
              <span className="bracket-detail-key">Best of</span>
              <span className="bracket-detail-value">{bracketSetDetails.bestOf}</span>
            </div>
            <div className="bracket-detail-row">
              <span className="bracket-detail-key">Replay attached</span>
              <span className="bracket-detail-value">
                {hasReplay ? "Yes" : "No"}
              </span>
            </div>
          </div>
          <div className="bracket-detail-card full">
            <div className="label">Controls</div>
            <div className="bracket-control-row">
              <button
                className="ghost-btn small"
                type="button"
                onClick={() => startMatchForSet(bracketSetDetails.id)}
                disabled={!bracketSetIsPending}
              >
                Start match
              </button>
              <button
                className="ghost-btn small"
                type="button"
                onClick={() => stepBracketSet(bracketSetDetails.id)}
                disabled={bracketSetIsCompleted}
              >
                Step
              </button>
              <button
                className="ghost-btn small"
                type="button"
                onClick={() => finalizeSetFromReference(bracketSetDetails.id)}
              >
                Finalize
              </button>
              <button
                className="ghost-btn small"
                type="button"
                onClick={() => resetSet(bracketSetDetails.id)}
              >
                Reset set
              </button>
            </div>
            {bracketSetActionStatus && (
              <div className="status-line">{bracketSetActionStatus}</div>
            )}
          </div>
          <div className="bracket-detail-card full">
            <div className="label">Replay Paths</div>
            {bracketSetDetailsStatus && (
              <div className="status-line">{bracketSetDetailsStatus}</div>
            )}
            {bracketSetReplayPaths.length === 0 && !bracketSetDetailsStatus && (
              <div className="muted tiny">No replay paths attached.</div>
            )}
            {bracketSetReplayPaths.length > 0 && (
              <ul className="bracket-replay-list">
                {bracketSetReplayPaths.map((path, idx) => {
                  const name = path.split(/[\\/]/).pop() ?? path;
                  const gameNumber = idx + 1;
                  return (
                    <li key={`${path}-${idx}`} className="bracket-replay-item">
                      <div className="bracket-replay-header">
                        <div>
                          <div className="bracket-replay-name">Game {gameNumber}</div>
                          <div className="muted tiny">{name}</div>
                        </div>
                        <button
                          className="ghost-btn small"
                          type="button"
                          onClick={() =>
                            streamBracketReplayGame(
                              bracketSetDetails.id,
                              path,
                              gameNumber,
                              bracketSetReplayPaths.length,
                            )
                          }
                          disabled={bracketSetIsPending}
                        >
                          Start game
                        </button>
                      </div>
                      <div className="muted tiny code bracket-replay-path">{path}</div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
          <div className="bracket-detail-card full">
            <div className="label">Slots</div>
            <div className="bracket-slot-grid">
              {bracketSetDetails.slots.map((slot, idx) => (
                <div key={`${bracketSetDetails.id}-detail-${idx}`} className="bracket-slot-card">
                  <div className="bracket-slot-name">{resolveSlotLabel(slot)}</div>
                  <div className="muted tiny code">{slot.slippiCode ?? "No code"}</div>
                  <div className="bracket-slot-meta">
                    {slot.score !== null && slot.score !== undefined && (
                      <span className="bracket-slot-pill">Score {slot.score}</span>
                    )}
                    {slot.result && (
                      <span className="bracket-slot-pill">{slot.result}</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
        <details className="bracket-raw">
          <summary>Raw JSON</summary>
          <pre>{bracketSetDetailsJson}</pre>
        </details>
      </div>
    </div>
  );
}
