import type { Setup, StartggSimSet, StartggSimSlot } from "../types/overlay";
import { stripSponsorTag } from "../tournamentUtils";

type SetupDetailsModalProps = {
  setupDetails: Setup;
  setupDetailsJson: string;
  setupOverlayUrl: string;
  overlayCopyStatus: string;
  setupDetailsExpectedOpponent: { tag?: string | null; code?: string | null } | null;
  resolveSlotLabel: (slot: StartggSimSlot) => string;
  copyOverlayUrl: (value: string) => Promise<void>;
  closeSetupDetails: () => void;
};

export default function SetupDetailsModal({
  setupDetails,
  setupDetailsJson,
  setupOverlayUrl,
  overlayCopyStatus,
  setupDetailsExpectedOpponent,
  resolveSlotLabel,
  copyOverlayUrl,
  closeSetupDetails,
}: SetupDetailsModalProps) {
  const stream = setupDetails.assignedStream ?? null;
  const startggSet: StartggSimSet | null = stream?.startggSet ?? null;

  // Determine status
  const status = stream
    ? stream.isPlaying
      ? "Playing"
      : "Idle"
    : "Unassigned";

  const statusClass = stream
    ? stream.isPlaying
      ? "playing"
      : "idle"
    : "unassigned";

  // Player names
  const p1Name = stripSponsorTag(stream?.p1Tag) || stream?.p1Code || "—";
  const p1Code = stream?.p1Code ?? null;
  const p2Name =
    stripSponsorTag(stream?.p2Tag) ||
    stream?.p2Code ||
    stripSponsorTag(setupDetailsExpectedOpponent?.tag) ||
    setupDetailsExpectedOpponent?.code ||
    "—";
  const p2Code = stream?.p2Code ?? setupDetailsExpectedOpponent?.code ?? null;
  const p2IsExpected = !stream?.p2Tag && !stream?.p2Code && setupDetailsExpectedOpponent;

  // Scores
  const p1Score = startggSet?.slots[0]?.score ?? null;
  const p2Score = startggSet?.slots[1]?.score ?? null;
  const hasScores = p1Score !== null || p2Score !== null;

  return (
    <div className="modal-backdrop" onClick={closeSetupDetails}>
      <div
        className="modal setup-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={`Setup ${setupDetails.id} details`}
      >
        {/* Header */}
        <div className="modal-header">
          <div className="setup-modal-title">
            <h2>{setupDetails.name}</h2>
            <span className={`setup-status-pill ${statusClass}`}>{status}</span>
          </div>
          <button className="icon-button" onClick={closeSetupDetails} aria-label="Close">
            ×
          </button>
        </div>

        {/* Match Display */}
        <div className="setup-match-display">
          <div className="setup-match-player left">
            <div className="setup-match-name">{p1Name}</div>
            {p1Code && <div className="setup-match-code">{p1Code}</div>}
            {hasScores && (
              <div className="setup-match-score">{p1Score ?? 0}</div>
            )}
          </div>
          <div className="setup-match-vs">vs</div>
          <div className="setup-match-player right">
            <div className="setup-match-name">
              {p2Name}
              {p2IsExpected && <span className="expected-badge">expected</span>}
            </div>
            {p2Code && <div className="setup-match-code">{p2Code}</div>}
            {hasScores && (
              <div className="setup-match-score">{p2Score ?? 0}</div>
            )}
          </div>
        </div>

        {/* Match Info */}
        {startggSet && (
          <div className="setup-match-info">
            <div className="setup-info-item">
              <span className="setup-info-label">Round</span>
              <span className="setup-info-value">{startggSet.roundLabel}</span>
            </div>
            <div className="setup-info-item">
              <span className="setup-info-label">Phase</span>
              <span className="setup-info-value">{startggSet.phaseName}</span>
            </div>
            <div className="setup-info-item">
              <span className="setup-info-label">Format</span>
              <span className="setup-info-value">Bo{startggSet.bestOf}</span>
            </div>
            <div className="setup-info-item">
              <span className="setup-info-label">State</span>
              <span className={`setup-info-value state-${startggSet.state}`}>
                {startggSet.state}
              </span>
            </div>
          </div>
        )}

        {!startggSet && stream && (
          <div className="setup-no-set">
            No Start.gg set linked yet
          </div>
        )}

        {!stream && (
          <div className="setup-no-set">
            No stream assigned to this setup
          </div>
        )}

        {/* Overlay URL */}
        <div className="setup-overlay-section">
          <div className="setup-info-label">Overlay URL</div>
          <div className="setup-overlay-row">
            <input
              className="setup-overlay-input"
              value={setupOverlayUrl}
              readOnly
              onFocus={(e) => e.currentTarget.select()}
            />
            <button
              className="ghost-btn small"
              onClick={() => copyOverlayUrl(setupOverlayUrl)}
              disabled={!setupOverlayUrl}
            >
              Copy
            </button>
          </div>
          {overlayCopyStatus && <div className="muted tiny">{overlayCopyStatus}</div>}
        </div>

        {/* Debug Info (collapsible) */}
        <details className="setup-debug">
          <summary>Debug Info</summary>
          <div className="setup-debug-grid">
            <div className="setup-debug-item">
              <span className="setup-debug-label">Setup ID</span>
              <span className="setup-debug-value">{setupDetails.id}</span>
            </div>
            <div className="setup-debug-item">
              <span className="setup-debug-label">Stream ID</span>
              <span className="setup-debug-value">{stream?.id ?? "—"}</span>
            </div>
            <div className="setup-debug-item">
              <span className="setup-debug-label">Source</span>
              <span className="setup-debug-value">{stream?.source ?? "—"}</span>
            </div>
            <div className="setup-debug-item">
              <span className="setup-debug-label">Window</span>
              <span className="setup-debug-value">{stream?.windowTitle ?? "—"}</span>
            </div>
            {startggSet && (
              <>
                <div className="setup-debug-item">
                  <span className="setup-debug-label">Set ID</span>
                  <span className="setup-debug-value">{startggSet.id}</span>
                </div>
                <div className="setup-debug-item">
                  <span className="setup-debug-label">Round #</span>
                  <span className="setup-debug-value">{startggSet.round}</span>
                </div>
              </>
            )}
          </div>
          <details className="setup-raw">
            <summary>Raw JSON</summary>
            <pre>{setupDetailsJson}</pre>
          </details>
        </details>
      </div>
    </div>
  );
}
