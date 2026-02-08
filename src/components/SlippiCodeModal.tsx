import { useState, useEffect, useRef } from "react";
import type { UnifiedEntrant } from "../types/overlay";
import { stripSponsorTag } from "../tournamentUtils";

type SlippiCodeModalProps = {
  entrant: UnifiedEntrant | undefined;
  onSave: (code: string | null) => void;
  onClose: () => void;
};

export default function SlippiCodeModal({
  entrant,
  onSave,
  onClose,
}: SlippiCodeModalProps) {
  const [code, setCode] = useState(entrant?.slippiCode || "");
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  // Close on escape
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  if (!entrant) {
    return null;
  }

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = code.trim();
    onSave(trimmed || null);
  };

  const handleClear = () => {
    setCode("");
    onSave(null);
  };

  const displayName = stripSponsorTag(entrant.name) || "Unknown";

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal-content slippi-code-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <h2>Edit Slippi Code</h2>
          <button className="close-btn" onClick={onClose} aria-label="Close">
            X
          </button>
        </div>

        <form onSubmit={handleSubmit}>
          <div className="modal-body">
            <div className="entrant-info">
              <span className="entrant-seed">#{entrant.seed}</span>
              <span className="entrant-name">{displayName}</span>
            </div>

            <label className="form-label">
              Slippi Code
              <input
                ref={inputRef}
                type="text"
                className="form-input"
                value={code}
                onChange={(e) => setCode(e.target.value.toUpperCase())}
                placeholder="ABCD#123"
                pattern="[A-Z0-9]+#[0-9]+"
              />
            </label>

            <div className="code-hint muted tiny">
              Format: TAG#123 (e.g., MANG#001)
            </div>
          </div>

          <div className="modal-footer">
            <button
              type="button"
              className="ghost-btn"
              onClick={handleClear}
            >
              Clear
            </button>
            <button type="button" className="ghost-btn" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="primary-btn">
              Save
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
