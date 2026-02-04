import type { AppConfig, BracketConfigInfo } from "../types/overlay";

const GEAR_ICON = (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M10 4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h6z" />
  </svg>
);

type SettingsModalProps = {
  config: AppConfig;
  configStatus: string;
  startggStatus: { text: string; tone: string } | null;
  bracketConfigs: BracketConfigInfo[];
  selectedBracketPath: string;
  startggLinkInputRef: React.RefObject<HTMLInputElement | null>;
  startggTokenInputRef: React.RefObject<HTMLInputElement | null>;
  updateConfig: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  saveConfig: () => Promise<void>;
  toggleTestMode: () => Promise<void>;
  browsePath: (key: keyof AppConfig, options: { directory: boolean; title: string }) => Promise<void>;
  handleBracketSelect: (path: string) => Promise<void>;
  openBracketWindow: () => void;
  spoofLiveGames: () => void;
  closeSettings: () => void;
};

export default function SettingsModal({
  config,
  configStatus,
  startggStatus,
  bracketConfigs,
  selectedBracketPath,
  startggLinkInputRef,
  startggTokenInputRef,
  updateConfig,
  saveConfig,
  toggleTestMode,
  browsePath,
  handleBracketSelect,
  openBracketWindow,
  spoofLiveGames,
  closeSettings,
}: SettingsModalProps) {
  return (
    <div className="modal-backdrop" onClick={closeSettings}>
      <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Settings">
        <div className="modal-header">
          <div>
            <p className="eyebrow">Settings</p>
          </div>
          <button className="icon-button" onClick={closeSettings} aria-label="Close settings">
            x
          </button>
        </div>
        <div className="settings-grid">
          <div className="settings-toggle">
            <div className="settings-label">Test mode</div>
            <button className="ghost-btn small" onClick={toggleTestMode}>
              {config.testMode ? "Disable" : "Enable"}
            </button>
          </div>
          <label className="settings-field">
            <span>Start.gg event link</span>
            <div className="path-input">
              <input
                ref={startggLinkInputRef}
                type="text"
                value={config.startggLink}
                onChange={(e) => updateConfig("startggLink", e.target.value)}
                placeholder="https://www.start.gg/tournament/.../event/..."
                spellCheck={false}
              />
            </div>
          </label>
          <label className="settings-field">
            <span>Start.gg API token</span>
            <div className="path-input">
              <input
                ref={startggTokenInputRef}
                type="password"
                value={config.startggToken}
                onChange={(e) => updateConfig("startggToken", e.target.value)}
                placeholder="Start.gg API token"
                spellCheck={false}
              />
            </div>
          </label>
          {!config.testMode && startggStatus && (
            <div className={`settings-note ${startggStatus.tone}`}>
              {startggStatus.text}
            </div>
          )}
          <div className="settings-toggle">
            <div className="settings-label">Start.gg polling</div>
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.startggPolling}
                onChange={(e) => updateConfig("startggPolling", e.target.checked)}
              />
              <span>Enabled</span>
            </label>
          </div>
          <div className="settings-toggle">
            <div className="settings-label">Auto stream</div>
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.autoStream}
                onChange={(e) => updateConfig("autoStream", e.target.checked)}
              />
              <span>Enabled</span>
            </label>
          </div>
          {config.testMode && (
            <>
              <label className="settings-field">
                <span>Test bracket</span>
                <select
                  className="ghost-select"
                  value={selectedBracketPath}
                  onChange={(e) => handleBracketSelect(e.target.value)}
                >
                  <option value="">Select bracket…</option>
                  {bracketConfigs.map((cfg) => (
                    <option key={cfg.path} value={cfg.path}>
                      {cfg.name}
                    </option>
                  ))}
                </select>
              </label>
              <div className="settings-toggle">
                <div className="settings-label">Test tools</div>
                <div className="action-row">
                  <button className="ghost-btn small" onClick={openBracketWindow}>
                    Open bracket
                  </button>
                  <button className="ghost-btn small" onClick={spoofLiveGames}>
                    Spoof live games
                  </button>
                </div>
              </div>
            </>
          )}
          <label className="settings-field">
            <span>Dolphin path</span>
            <div className="path-input">
              <input
                type="text"
                value={config.dolphinPath}
                onChange={(e) => updateConfig("dolphinPath", e.target.value)}
                placeholder="/path/to/dolphin"
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button small"
                onClick={() => browsePath("dolphinPath", { directory: false, title: "Select Dolphin binary" })}
                aria-label="Browse for Dolphin path"
              >
                {GEAR_ICON}
              </button>
            </div>
          </label>
          <label className="settings-field">
            <span>Melee ISO path</span>
            <div className="path-input">
              <input
                type="text"
                value={config.ssbmIsoPath}
                onChange={(e) => updateConfig("ssbmIsoPath", e.target.value)}
                placeholder="/path/to/melee.iso"
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button small"
                onClick={() => browsePath("ssbmIsoPath", { directory: false, title: "Select Melee ISO" })}
                aria-label="Browse for Melee ISO path"
              >
                {GEAR_ICON}
              </button>
            </div>
          </label>
          <label className="settings-field">
            <span>Slippi launcher path</span>
            <div className="path-input">
              <input
                type="text"
                value={config.slippiLauncherPath}
                onChange={(e) => updateConfig("slippiLauncherPath", e.target.value)}
                placeholder="/path/to/slippi.AppImage"
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button small"
                onClick={() => browsePath("slippiLauncherPath", { directory: false, title: "Select Slippi Launcher" })}
                aria-label="Browse for Slippi Launcher path"
              >
                {GEAR_ICON}
              </button>
            </div>
          </label>
          <label className="settings-field">
            <span>Spectate folder path</span>
            <div className="path-input">
              <input
                type="text"
                value={config.spectateFolderPath}
                onChange={(e) => updateConfig("spectateFolderPath", e.target.value)}
                placeholder="/path/to/spectate"
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button small"
                onClick={() => browsePath("spectateFolderPath", { directory: true, title: "Select Spectate folder" })}
                aria-label="Browse for Spectate folder"
              >
                {GEAR_ICON}
              </button>
            </div>
          </label>
        </div>
        <div className="modal-actions">
          {configStatus && <div className="modal-status">{configStatus}</div>}
          <button className="ghost-btn" onClick={() => saveConfig()}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
