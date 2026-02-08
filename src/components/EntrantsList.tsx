import type { UnifiedEntrant } from "../types/overlay";
import { stripSponsorTag } from "../tournamentUtils";

type EntrantsListProps = {
  entrants: UnifiedEntrant[];
  selectedEntrantId: number | null;
  onSelectEntrant: (entrantId: number) => void;
  onEditCode: (entrantId: number) => void;
};

export default function EntrantsList({
  entrants,
  selectedEntrantId,
  onSelectEntrant,
  onEditCode,
}: EntrantsListProps) {
  if (entrants.length === 0) {
    return <div className="muted tiny">No entrants loaded.</div>;
  }

  return (
    <div className="entrants-list">
      {entrants.map((entrant) => (
        <EntrantRow
          key={entrant.id}
          entrant={entrant}
          isSelected={selectedEntrantId === entrant.id}
          onSelect={() => onSelectEntrant(entrant.id)}
          onEditCode={() => onEditCode(entrant.id)}
        />
      ))}
    </div>
  );
}

type EntrantRowProps = {
  entrant: UnifiedEntrant;
  isSelected: boolean;
  onSelect: () => void;
  onEditCode: () => void;
};

function formatGameStatus(entrant: UnifiedEntrant): string | null {
  const game = entrant.currentGame;
  if (!game) return null;

  const parts: string[] = [];

  if (game.roundLabel) {
    parts.push(game.roundLabel);
  }

  if (game.opponentName) {
    parts.push(`vs ${stripSponsorTag(game.opponentName)}`);
  }

  if (game.gameNumber && game.bestOf) {
    parts.push(`Game ${game.gameNumber}/${game.bestOf}`);
  } else if (game.gameNumber) {
    parts.push(`Game ${game.gameNumber}`);
  }

  if (game.scores) {
    parts.push(`(${game.scores[0]}-${game.scores[1]})`);
  }

  return parts.length > 0 ? parts.join(" \u00B7 ") : null;
}

function EntrantRow({ entrant, isSelected, onSelect, onEditCode }: EntrantRowProps) {
  const displayName = stripSponsorTag(entrant.name) || "Unknown";
  const displayCode = entrant.slippiCode || "No code";
  const gameStatus = formatGameStatus(entrant);

  return (
    <article
      className={`entrant-row ${isSelected ? "selected" : ""} ${entrant.assignedSetupId ? "assigned" : ""}`}
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.currentTarget !== event.target) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="entrant-main">
        <span className="entrant-seed">#{entrant.seed}</span>
        <span className="entrant-name">{displayName}</span>
        <button
          className="entrant-code ghost-btn tiny"
          onClick={(e) => {
            e.stopPropagation();
            onEditCode();
          }}
          title="Edit slippi code"
        >
          {displayCode}
        </button>
      </div>
      {gameStatus && (
        <div className="entrant-game-status">{gameStatus}</div>
      )}
      <div className="entrant-badges">
        {entrant.isStreaming && (
          <span className="entrant-badge streaming" title="Broadcasting">
            <StreamingIcon />
          </span>
        )}
        {entrant.isPlaying && (
          <span className="entrant-badge playing" title="In game">
            <PlayingIcon />
          </span>
        )}
        {entrant.assignedSetupId && (
          <span className="entrant-badge assigned" title={`Setup ${entrant.assignedSetupId}`}>
            S{entrant.assignedSetupId}
          </span>
        )}
        {entrant.bracketState === "eliminated" && (
          <span className="entrant-badge eliminated" title="Eliminated">
            Out
          </span>
        )}
        {entrant.bracketState === "winner" && (
          <span className="entrant-badge winner" title="Winner">
            W
          </span>
        )}
      </div>
    </article>
  );
}

function StreamingIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8z" opacity="0.3" />
    </svg>
  );
}

function PlayingIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
      <path d="M8 5v14l11-7z" />
    </svg>
  );
}
