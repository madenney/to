export type Setup = {
  id: number;
  name: string;
  assignedStream?: SlippiStream | null;
};

export type SlippiStream = {
  id: string;
  windowTitle?: string | null;
  p1Tag?: string | null;
  p2Tag?: string | null;
  p1Code?: string | null;
  p2Code?: string | null;
  source?: string | null;
};

export type SlippiWindowInfo = {
  id: number;
  title?: string | null;
  x: i32;
  y: i32;
  width: u32;
  height: u32;
  screen?: number;
};

export type AppConfig = {
  dolphinPath: string;
  ssbmIsoPath: string;
  slippiLauncherPath: string;
  spectateFolderPath: string;
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

export type StartggSimSlot = {
  entrantId?: number | null;
  entrantName?: string | null;
  slippiCode?: string | null;
  seed?: number | null;
  score?: number | null;
  result?: string | null;
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
