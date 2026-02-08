export type Setup = {
  id: number;
  name: string;
  assignedStream?: SlippiStream | null;
};

export type AssignStreamResult = {
  setups: Setup[];
  warning?: string | null;
};

export type SlippiStream = {
  id: string;
  windowTitle?: string | null;
  p1Tag?: string | null;
  p2Tag?: string | null;
  p1Code?: string | null;
  p2Code?: string | null;
  startggEntrantId?: number | null;
  replayPath?: string | null;
  isPlaying?: boolean | null;
  source?: string | null;
  startggSet?: StartggSimSet | null;
};

export type SlippiWindowInfo = {
  id: number;
  title?: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  screen?: number;
};

export type AppConfig = {
  dolphinPath: string;
  ssbmIsoPath: string;
  slippiLauncherPath: string;
  spectateFolderPath: string;
  startggLink: string;
  startggToken: string;
  startggPolling: boolean;
  autoStream: boolean;
  testMode: boolean;
  testBracketPath: string;
  autoCompleteBracket: boolean;
};

export type StartggSimEvent = {
  id: string;
  name: string;
  slug: string;
};

export type StartggSimPhase = {
  id: string;
  name: string;
  bestOf: number;
};

export type StartggSimEntrant = {
  id: number;
  name: string;
  seed: number;
  slippiCode: string;
};

export type BroadcastPlayerSelection = {
  id: number;
  name: string;
  slippiCode: string;
};

export type StartggSimSlot = {
  entrantId?: number | null;
  entrantName?: string | null;
  slippiCode?: string | null;
  seed?: number | null;
  score?: number | null;
  result?: string | null;
  sourceType?: string | null;
  sourceSetId?: number | null;
  sourceLabel?: string | null;
};

export type StartggSimSet = {
  id: number;
  phaseId: string;
  phaseName: string;
  round: number;
  roundLabel: string;
  bestOf: number;
  state: string;
  startedAtMs?: number | null;
  completedAtMs?: number | null;
  updatedAtMs: number;
  winnerId?: number | null;
  slots: StartggSimSlot[];
};

export type StartggSimState = {
  event: StartggSimEvent;
  phases: StartggSimPhase[];
  entrants: StartggSimEntrant[];
  sets: StartggSimSet[];
  startedAtMs: number;
  nowMs: number;
  eventLink?: string | null;
};

export type StartggLiveSnapshot = {
  state?: StartggSimState | null;
  lastError?: string | null;
  lastFetchMs?: number | null;
};

export type BracketConfigInfo = {
  name: string;
  path: string;
};

export type SpoofReplayResult = {
  started: number;
  missing: number;
};

export type ReplayStreamUpdate = {
  type: "start" | "progress" | "complete" | "error";
  setId?: number | null;
  replayIndex?: number | null;
  replayTotal?: number | null;
  replayPath?: string | null;
  outputPath?: string | null;
  frame?: number | null;
  totalFrames?: number | null;
  fps?: number | null;
  message?: string | null;
};

// Unified Entrant types for entrant-centric architecture

export type EntrantBracketState = "active" | "eliminated" | "winner";

export type LiveGameInfo = {
  stage?: string | null;
  character: string;
  opponentCode?: string | null;
  opponentName?: string | null;
  roundLabel?: string | null;
  bestOf?: number | null;
  gameNumber?: number | null;
  scores?: [number, number] | null;
};

export type UnifiedEntrant = {
  // Start.gg data (primary source)
  id: number;
  name: string;
  seed: number;
  slippiCode?: string | null;
  team?: string | null;
  currentSetId?: number | null;
  bracketState: EntrantBracketState;

  // Slippi App data (streaming status)
  isStreaming: boolean;

  // Spectate folder data (playing status)
  isPlaying: boolean;
  currentGame?: LiveGameInfo | null;

  // Assignment
  assignedSetupId?: number | null;
  autoAssigned: boolean;
};

export type SetupWithSeed = {
  id: number;
  name: string;
  assignedEntrantIds: number[];
  highestSeed?: number | null;
  isAvailable: boolean;
};
