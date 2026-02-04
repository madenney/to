#!/usr/bin/env node
import { createRequire } from "module";
import path from "path";

const require = createRequire(import.meta.url);
const fs = require("fs");
const {
  Command,
  SlpFileWriter,
  SlpFileWriterEvent,
} = require("@slippi/slippi-js/node");

function getArg(flag) {
  const idx = process.argv.indexOf(flag);
  if (idx === -1 || idx + 1 >= process.argv.length) return null;
  return process.argv[idx + 1];
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

function uniqueReplayFilename(meta) {
  const parts = ["Game", Date.now()];
  if (meta?.setId != null) {
    parts.push(`set${meta.setId}`);
  }
  if (meta?.replayIndex != null) {
    parts.push(`g${meta.replayIndex}`);
  }
  parts.push(Math.random().toString(36).slice(2, 8));
  return `${parts.join("_")}.slp`;
}

function createFileRef(filePath) {
  const fd = fs.openSync(filePath, "r");
  const stat = fs.fstatSync(fd);
  return {
    size: () => stat.size,
    read: (buffer, offset, length, position) =>
      fs.readSync(fd, buffer, offset, length, position),
    close: () => fs.closeSync(fd),
  };
}

function getRawDataPosition(ref) {
  const buffer = Buffer.alloc(1);
  ref.read(buffer, 0, buffer.length, 0);
  if (buffer[0] === 0x36) {
    return 0;
  }
  if (buffer[0] !== "{".charCodeAt(0)) {
    return 0;
  }
  return 15;
}

function getRawDataLength(ref, position) {
  const fileSize = ref.size();
  if (position === 0) {
    return fileSize;
  }
  const buffer = Buffer.alloc(4);
  ref.read(buffer, 0, buffer.length, position - 4);
  const rawDataLen =
    (buffer[0] << 24) |
    (buffer[1] << 16) |
    (buffer[2] << 8) |
    buffer[3];
  if (rawDataLen > 0) {
    return rawDataLen;
  }
  return fileSize - position;
}

function getMessageSizes(ref, position) {
  const messageSizes = {};
  if (position === 0) {
    messageSizes[0x36] = 0x140;
    messageSizes[0x37] = 0x6;
    messageSizes[0x38] = 0x46;
    messageSizes[0x39] = 0x1;
    return messageSizes;
  }

  const header = Buffer.alloc(2);
  ref.read(header, 0, header.length, position);
  if (header[0] !== Command.MESSAGE_SIZES) {
    return messageSizes;
  }
  const payloadLength = header[1];
  messageSizes[Command.MESSAGE_SIZES] = payloadLength;

  const sizesBuf = Buffer.alloc(payloadLength - 1);
  ref.read(sizesBuf, 0, sizesBuf.length, position + 2);
  for (let i = 0; i < payloadLength - 1; i += 3) {
    const command = sizesBuf[i];
    const size = (sizesBuf[i + 1] << 8) | sizesBuf[i + 2];
    messageSizes[command] = size;
  }
  return messageSizes;
}

function openSlpFile(filePath) {
  const ref = createFileRef(filePath);
  const rawDataPosition = getRawDataPosition(ref);
  const rawDataLength = getRawDataLength(ref, rawDataPosition);
  const messageSizes = getMessageSizes(ref, rawDataPosition);
  return {
    ref,
    rawDataPosition,
    rawDataLength,
    messageSizes,
  };
}

function closeSlpFile(slpFile) {
  if (slpFile?.ref?.close) {
    slpFile.ref.close();
  }
}

function readInt32(buffer, offset) {
  if (offset + 4 > buffer.length) {
    return null;
  }
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  return view.getInt32(offset);
}

function readUint16(buffer, offset) {
  if (offset + 2 > buffer.length) {
    return null;
  }
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  return view.getUint16(offset);
}

function readUint8(buffer, offset) {
  if (offset + 1 > buffer.length) {
    return null;
  }
  return buffer[offset];
}

function readBool(buffer, offset) {
  const value = readUint8(buffer, offset);
  return value === null ? null : Boolean(value);
}

function extractFrame(command, buffer) {
  switch (command) {
    case Command.FRAME_START:
    case Command.PRE_FRAME_UPDATE:
    case Command.POST_FRAME_UPDATE:
    case Command.ITEM_UPDATE:
    case Command.FRAME_BOOKEND:
      return readInt32(buffer, 0x1);
    default:
      return null;
  }
}

function iterateEvents(slpFile, callback, startPos) {
  const ref = slpFile.ref;
  let readPosition =
    startPos != null && startPos > 0 ? startPos : slpFile.rawDataPosition;
  const stopReadingAt = slpFile.rawDataPosition + slpFile.rawDataLength;

  const commandPayloadBuffers = {};
  Object.entries(slpFile.messageSizes).forEach(([command, size]) => {
    commandPayloadBuffers[command] = Buffer.alloc(size + 1);
  });

  let splitMessageBuffer = Buffer.alloc(0);
  const commandByteBuffer = Buffer.alloc(1);

  while (readPosition < stopReadingAt) {
    ref.read(commandByteBuffer, 0, 1, readPosition);
    let commandByte = commandByteBuffer[0] ?? 0;
    let buffer = commandPayloadBuffers[commandByte];
    if (!buffer) {
      return readPosition;
    }

    if (buffer.length > stopReadingAt - readPosition) {
      return readPosition;
    }

    const advanceAmount = buffer.length;
    ref.read(buffer, 0, buffer.length, readPosition);

    if (commandByte === Command.SPLIT_MESSAGE) {
      const size = readUint16(buffer, 0x201) ?? 512;
      const isLastMessage = readBool(buffer, 0x204);
      const internalCommand = readUint8(buffer, 0x203) ?? 0;

      if (splitMessageBuffer.length === 0) {
        splitMessageBuffer = Buffer.alloc(1);
        splitMessageBuffer[0] = internalCommand;
      }

      const appendBuf = buffer.subarray(0x1, 0x1 + size);
      const merged = Buffer.alloc(splitMessageBuffer.length + appendBuf.length);
      splitMessageBuffer.copy(merged, 0);
      appendBuf.copy(merged, splitMessageBuffer.length);
      splitMessageBuffer = merged;

      if (isLastMessage) {
        commandByte = splitMessageBuffer[0] ?? 0;
        buffer = splitMessageBuffer;
        splitMessageBuffer = Buffer.alloc(0);
      } else {
        readPosition += advanceAmount;
        continue;
      }
    }

    const payload = { frame: extractFrame(commandByte, buffer) };
    const shouldStop = callback(commandByte, payload, buffer);
    if (shouldStop) {
      break;
    }

    readPosition += advanceAmount;
  }

  return readPosition;
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

async function waitUntil(startTimeMs) {
  if (!Number.isFinite(Number(startTimeMs))) {
    return;
  }
  const delay = Number(startTimeMs) - Date.now();
  if (delay > 0) {
    await sleep(delay);
  }
}

async function streamReplay(task, index, defaultFps) {
  const replayPath = task.replayPath;
  const outputDir = task.outputDir;
  if (!replayPath || !outputDir) {
    throw new Error("Task is missing replayPath or outputDir.");
  }
  fs.mkdirSync(outputDir, { recursive: true });
  const fps = Number(task.fps || defaultFps || 60);
  const frameMs = 1000 / fps;
  await waitUntil(task.startTimeMs);

  const replayIndex = Number(task.replayIndex || index + 1);
  const replayTotal = Number(task.replayTotal || 0);
  const setId = Number.isFinite(Number(task.setId)) ? Number(task.setId) : null;
  const metaBase = {
    setId,
    replayIndex,
    replayTotal: replayTotal > 0 ? replayTotal : null,
    replayPath,
    fps,
  };
  const replayFilename = uniqueReplayFilename(metaBase);

  try {
    const { events, maxFrame } = collectEvents(replayPath);
    const totalFrames = typeof maxFrame === "number" ? maxFrame : null;
    let outputPath = null;
    let startEmitted = false;

    const fileWriter = new SlpFileWriter({
      folderPath: outputDir,
      newFilename: (folder) => path.join(folder, replayFilename),
    });
    const fileCompletePromise = new Promise((resolve) => {
      fileWriter.once(SlpFileWriterEvent.FILE_COMPLETE, (filePath) => {
        outputPath = filePath || outputPath;
        resolve(filePath);
      });
    });

    fileWriter.once(SlpFileWriterEvent.NEW_FILE, (filePath) => {
      outputPath = filePath;
      startEmitted = true;
      emitProgress({
        type: "start",
        totalFrames,
        outputPath,
        ...metaBase,
      });
    });

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
      fileWriter.write(event.buffer);
      if (currentFrame !== null && Date.now() - lastEmitAt >= 1000) {
        emitProgress({
          type: "progress",
          frame: currentFrame,
          totalFrames,
          outputPath,
          ...metaBase,
        });
        lastEmitAt = Date.now();
      }
    }

    fileWriter.endCurrentFile();
    await fileCompletePromise;
    if (!startEmitted) {
      emitProgress({
        type: "start",
        totalFrames,
        outputPath,
        ...metaBase,
      });
    }
    emitProgress({
      type: "complete",
      frame: currentFrame,
      totalFrames,
      outputPath,
      ...metaBase,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    emitProgress({
      type: "error",
      message,
      ...metaBase,
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
  const gapMs = Number(payload.gapMs || 0);

  if (!Array.isArray(tasks) || tasks.length === 0) {
    throw new Error("No tasks provided.");
  }

  if (sequential) {
    for (let i = 0; i < tasks.length; i += 1) {
      await streamReplay(tasks[i], i, fps);
      if (gapMs > 0 && i < tasks.length - 1) {
        await sleep(gapMs);
      }
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
