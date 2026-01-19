#!/usr/bin/env node
import { createRequire } from "module";

const require = createRequire(import.meta.url);
const fs = require("fs");
const path = require("path");
const {
  openSlpFile,
  iterateEvents,
  closeSlpFile,
  SlpFile,
} = require("@slippi/slippi-js");

function getArg(flag) {
  const idx = process.argv.indexOf(flag);
  if (idx === -1 || idx + 1 >= process.argv.length) return null;
  return process.argv[idx + 1];
}

function pad(value) {
  return String(value).padStart(2, "0");
}

function formatGameName(date) {
  const year = date.getFullYear();
  const month = pad(date.getMonth() + 1);
  const day = pad(date.getDate());
  const hours = pad(date.getHours());
  const minutes = pad(date.getMinutes());
  const seconds = pad(date.getSeconds());
  return `Game_${year}${month}${day}T${hours}${minutes}${seconds}.slp`;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function emitProgress(event) {
  try {
    process.stdout.write(`SPOOF_PROGRESS:${JSON.stringify(event)}\n`);
  } catch (err) {
    // If stdout is not writable, ignore to avoid crashing the stream.
  }
}

function collectEvents(replayPath) {
  const slp = openSlpFile(replayPath);
  const events = [];
  let maxFrame = null;
  iterateEvents(slp, (command, payload, buffer) => {
    const frame =
      payload && typeof payload.frame === "number" ? payload.frame : null;
    if (typeof frame === "number") {
      if (maxFrame === null || frame > maxFrame) {
        maxFrame = frame;
      }
    }
    events.push({
      frame,
      buffer: Buffer.from(buffer),
    });
    return false;
  });
  closeSlpFile(slp);
  return { events, maxFrame };
}

async function streamReplay(task, index, defaultFps) {
  const replayPath = task.replayPath;
  const outputDir = task.outputDir;
  if (!replayPath || !outputDir) {
    throw new Error("Task is missing replayPath or outputDir.");
  }
  const fps = Number(task.fps || defaultFps || 60);
  const frameMs = 1000 / fps;
  const startTime = task.startTimeMs ? new Date(task.startTimeMs) : new Date();
  let outputName = formatGameName(startTime);
  let outputPath = path.join(outputDir, outputName);
  if (fs.existsSync(outputPath)) {
    outputName = outputName.replace(".slp", `_${index + 1}.slp`);
    outputPath = path.join(outputDir, outputName);
  }

  const replayIndex = Number(task.replayIndex || index + 1);
  const replayTotal = Number(task.replayTotal || 0);
  const setId = Number.isFinite(Number(task.setId)) ? Number(task.setId) : null;
  const meta = {
    setId,
    replayIndex,
    replayTotal: replayTotal > 0 ? replayTotal : null,
    replayPath,
    outputPath,
    fps,
  };

  try {
    const { events, maxFrame } = collectEvents(replayPath);
    const totalFrames = typeof maxFrame === "number" ? maxFrame : null;
    emitProgress({
      type: "start",
      totalFrames,
      ...meta,
    });

    const slpFile = new SlpFile(outputPath);
    if (typeof slpFile.setMetadata === "function") {
      slpFile.setMetadata({ startTime });
    }
    let lastFrame = null;
    let currentFrame = null;
    let lastEmitAt = Date.now();

    for (const event of events) {
      if (typeof event.frame === "number") {
        currentFrame = event.frame;
        if (lastFrame === null) {
          lastFrame = event.frame;
        } else if (event.frame > lastFrame) {
          const delta = event.frame - lastFrame;
          await sleep(delta * frameMs);
          lastFrame = event.frame;
        }
      }
      slpFile.write(event.buffer);
      if (currentFrame !== null && Date.now() - lastEmitAt >= 1000) {
        emitProgress({
          type: "progress",
          frame: currentFrame,
          totalFrames,
          ...meta,
        });
        lastEmitAt = Date.now();
      }
    }

    slpFile.end();
    emitProgress({
      type: "complete",
      frame: currentFrame,
      totalFrames,
      ...meta,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    emitProgress({
      type: "error",
      message,
      ...meta,
    });
    throw err;
  }
}

async function main() {
  const tasksPath = getArg("--tasks");
  if (!tasksPath) {
    throw new Error("Usage: spoof_live_games.js --tasks <tasks.json>");
  }
  const raw = fs.readFileSync(tasksPath, "utf8");
  const payload = JSON.parse(raw);
  const tasks = Array.isArray(payload.streams) ? payload.streams : payload;
  const fps = payload.fps;
  const sequential = payload.sequential === true;

  if (!Array.isArray(tasks) || tasks.length === 0) {
    throw new Error("No tasks provided.");
  }

  if (sequential) {
    for (let i = 0; i < tasks.length; i += 1) {
      await streamReplay(tasks[i], i, fps);
    }
  } else {
    await Promise.all(
      tasks.map((task, index) => streamReplay(task, index, fps))
    );
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
