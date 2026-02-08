import type { AppConfig } from "../types/overlay";

const BRACKET_ZOOM_MIN = 0.5;
const BRACKET_ZOOM_MAX = 1.5;
const BRACKET_ZOOM_STEP = 0.05;

type BracketSettingsModalProps = {
  config: AppConfig;
  bracketZoom: number;
  isRefreshing: boolean;
  setBracketZoom: (zoom: number) => void;
  setAutoCompleteBracket: (enabled: boolean) => void;
  resetBracketState: () => Promise<void>;
  refreshBracketState: () => Promise<void>;
  closeBracketSettings: () => void;
};

export default function BracketSettingsModal({
  config,
  bracketZoom,
  isRefreshing,
  setBracketZoom,
  setAutoCompleteBracket,
  resetBracketState,
  refreshBracketState,
  closeBracketSettings,
}: BracketSettingsModalProps) {
  return (
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
              <button className="ghost-btn small" onClick={() => refreshBracketState()} disabled={isRefreshing}>
                {isRefreshing ? <><span className="btn-spinner" /> Refreshing…</> : "Refresh"}
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
  );
}
