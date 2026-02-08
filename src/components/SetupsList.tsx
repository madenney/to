import { useMemo } from "react";
import type { SetupWithSeed, UnifiedEntrant } from "../types/overlay";
import { stripSponsorTag } from "../tournamentUtils";

type SetupsListProps = {
  setups: SetupWithSeed[];
  entrants: UnifiedEntrant[];
  selectedEntrantId: number | null;
  onSetupClick: (setupId: number) => void;
  onUnassign: (setupId: number) => void;
};

export default function SetupsList({
  setups,
  entrants,
  selectedEntrantId,
  onSetupClick,
  onUnassign,
}: SetupsListProps) {
  // Create a lookup map for entrants
  const entrantMap = useMemo(
    () => new Map(entrants.map((e) => [e.id, e])),
    [entrants]
  );

  if (setups.length === 0) {
    return <div className="muted tiny">No setups configured.</div>;
  }

  return (
    <div className="setups-list">
      {setups.map((setup) => (
        <SetupCard
          key={setup.id}
          setup={setup}
          entrantMap={entrantMap}
          isSelectionActive={selectedEntrantId !== null}
          onSetupClick={() => onSetupClick(setup.id)}
          onUnassign={() => onUnassign(setup.id)}
        />
      ))}
    </div>
  );
}

type SetupCardProps = {
  setup: SetupWithSeed;
  entrantMap: Map<number, UnifiedEntrant>;
  isSelectionActive: boolean;
  onSetupClick: () => void;
  onUnassign: () => void;
};

function SetupCard({
  setup,
  entrantMap,
  isSelectionActive,
  onSetupClick,
  onUnassign,
}: SetupCardProps) {
  const assignedEntrants = setup.assignedEntrantIds
    .map((id) => entrantMap.get(id))
    .filter((e): e is UnifiedEntrant => e !== undefined);

  const hasAssignments = assignedEntrants.length > 0;
  const isAvailable = setup.isAvailable;

  // Find if any assigned entrant is currently playing
  const isPlaying = assignedEntrants.some((e) => e.isPlaying);
  const isStreaming = assignedEntrants.some((e) => e.isStreaming);

  return (
    <article
      className={`setup-card-v2 ${isSelectionActive && isAvailable ? "selectable" : ""} ${hasAssignments ? "has-assignments" : ""}`}
      role="button"
      tabIndex={0}
      onClick={onSetupClick}
      onKeyDown={(event) => {
        if (event.currentTarget !== event.target) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSetupClick();
        }
      }}
    >
      <div className="setup-header-v2">
        <span className="setup-name-v2">{setup.name}</span>
        {setup.highestSeed && (
          <span className="setup-seed-badge" title="Best seed on this setup">
            #{setup.highestSeed}
          </span>
        )}
        {hasAssignments && (
          <button
            className="ghost-btn tiny"
            onClick={(e) => {
              e.stopPropagation();
              onUnassign();
            }}
          >
            Clear
          </button>
        )}
      </div>

      <div className="setup-assignments">
        {hasAssignments ? (
          assignedEntrants.map((entrant, index) => (
            <div key={entrant.id} className="setup-player">
              <span className="setup-player-label">P{index + 1}</span>
              <span className="setup-player-name">
                {stripSponsorTag(entrant.name) || "Unknown"}
              </span>
              <span className="setup-player-code muted">
                {entrant.slippiCode || "N/A"}
              </span>
            </div>
          ))
        ) : (
          <div className="setup-empty muted">Unassigned</div>
        )}
      </div>

      <div className="setup-status-badges">
        {isStreaming && (
          <span className="setup-status-badge streaming">Broadcasting</span>
        )}
        {isPlaying && (
          <span className="setup-status-badge playing">In game</span>
        )}
        {!hasAssignments && isSelectionActive && (
          <span className="setup-status-badge available">Click to assign</span>
        )}
      </div>
    </article>
  );
}
