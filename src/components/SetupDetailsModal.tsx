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
  const setupDetailsStream = setupDetails.assignedStream ?? null;
  const setupDetailsStartggSet: StartggSimSet | null = setupDetailsStream?.startggSet ?? null;

  return (
    <div className="modal-backdrop" onClick={closeSetupDetails}>
      <div
        className="modal setup-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={`Setup ${setupDetails.id} details`}
      >
        <div className="modal-header">
          <div>
            <p className="eyebrow">Setup {setupDetails.id}</p>
            <div className="setup-detail-title">{setupDetails.name}</div>
          </div>
          <button className="icon-button" onClick={closeSetupDetails} aria-label="Close setup details">
            x
          </button>
        </div>
        <div className="setup-detail-grid">
          <div className="setup-detail-card">
            <div className="label">Setup</div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">ID</span>
              <span className="setup-detail-value">#{setupDetails.id}</span>
            </div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">Name</span>
              <span className="setup-detail-value">{setupDetails.name}</span>
            </div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">Assigned</span>
              <span className="setup-detail-value">
                {setupDetailsStream ? "Yes" : "No"}
              </span>
            </div>
          </div>
          <div className="setup-detail-card">
            <div className="label">Stream</div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">ID</span>
              <span className="setup-detail-value">
                {setupDetailsStream?.id ?? "None"}
              </span>
            </div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">Source</span>
              <span className="setup-detail-value">
                {setupDetailsStream?.source ?? "Unknown"}
              </span>
            </div>
            <div className="setup-detail-row">
              <span className="setup-detail-key">Status</span>
              <span className="setup-detail-value">
                {setupDetailsStream
                  ? setupDetailsStream.isPlaying
                    ? "Playing"
                    : "Idle"
                  : "N/A"}
              </span>
            </div>
            {setupDetailsStream?.windowTitle && (
              <div className="setup-detail-row">
                <span className="setup-detail-key">Window</span>
                <span className="setup-detail-value">
                  {setupDetailsStream.windowTitle}
                </span>
              </div>
            )}
          </div>
          <div className="setup-detail-card full">
            <div className="label">Overlay</div>
            <div className="setup-overlay-row">
              <input
                className="setup-overlay-input"
                value={setupOverlayUrl}
                readOnly
                onFocus={(event) => event.currentTarget.select()}
                aria-label={`Setup ${setupDetails.id} overlay URL`}
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
          <div className="setup-detail-card full">
            <div className="label">Players</div>
            <div className="setup-player-grid">
              <div className="setup-player-card">
                <div className="setup-player-label">P1 (Broadcast)</div>
                <div className="setup-player-name">
                  {stripSponsorTag(setupDetailsStream?.p1Tag) ||
                    setupDetailsStream?.p1Code ||
                    "Waiting"}
                </div>
                <div className="muted tiny code">
                  {setupDetailsStream?.p1Code ?? "N/A"}
                </div>
              </div>
              <div className="setup-player-card right">
                <div className="setup-player-label">P2</div>
                <div className="setup-player-name">
                  {stripSponsorTag(setupDetailsStream?.p2Tag) ||
                    setupDetailsStream?.p2Code ||
                    stripSponsorTag(setupDetailsExpectedOpponent?.tag) ||
                    setupDetailsExpectedOpponent?.code ||
                    "Waiting"}
                </div>
                <div className="muted tiny code">
                  {setupDetailsStream?.p2Code ??
                    setupDetailsExpectedOpponent?.code ??
                    "N/A"}
                </div>
              </div>
            </div>
            {setupDetailsExpectedOpponent && (
              <div className="setup-detail-row">
                <span className="setup-detail-key">Expected</span>
                <span className="setup-detail-value">
                  {stripSponsorTag(setupDetailsExpectedOpponent.tag) ||
                    setupDetailsExpectedOpponent.code ||
                    "Unknown"}
                </span>
              </div>
            )}
          </div>
          <div className="setup-detail-card full">
            <div className="label">Start.gg Set</div>
            {setupDetailsStartggSet ? (
              <>
                <div className="setup-detail-row">
                  <span className="setup-detail-key">Round</span>
                  <span className="setup-detail-value">
                    {setupDetailsStartggSet.roundLabel}
                  </span>
                </div>
                <div className="setup-detail-row">
                  <span className="setup-detail-key">Phase</span>
                  <span className="setup-detail-value">
                    {setupDetailsStartggSet.phaseName}
                  </span>
                </div>
                <div className="setup-detail-row">
                  <span className="setup-detail-key">State</span>
                  <span className="setup-detail-value">
                    {setupDetailsStartggSet.state}
                  </span>
                </div>
                <div className="setup-detail-row">
                  <span className="setup-detail-key">Best of</span>
                  <span className="setup-detail-value">
                    {setupDetailsStartggSet.bestOf}
                  </span>
                </div>
                <div className="setup-set-slots">
                  {setupDetailsStartggSet.slots.map((slot, idx) => (
                    <div key={`${setupDetails.id}-slot-${idx}`} className="setup-set-slot">
                      <div className="setup-set-slot-name">{resolveSlotLabel(slot)}</div>
                      <div className="muted tiny code">{slot.slippiCode ?? "No code"}</div>
                      <div className="setup-set-slot-meta">
                        {slot.score !== null && slot.score !== undefined && (
                          <span className="setup-slot-pill">Score {slot.score}</span>
                        )}
                        {slot.result && (
                          <span className="setup-slot-pill">{slot.result}</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div className="muted tiny">No Start.gg set attached yet.</div>
            )}
          </div>
        </div>
        <details className="setup-raw">
          <summary>Raw JSON</summary>
          <pre>{setupDetailsJson}</pre>
        </details>
      </div>
    </div>
  );
}
