#!/usr/bin/env bun

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(scriptDir, "..");
const repoRoot = resolve(webRoot, "../..");
const sourcePath = resolve(repoRoot, "skills/agentics-introduction/SKILL.md");
const destinationPath = resolve(webRoot, "public/skill.md");

try {
  const source = await readFile(sourcePath);
  let existing = null;
  try {
    existing = await readFile(destinationPath);
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error;
    }
  }

  if (existing?.equals(source)) {
    console.log(
      "Agentics introduction skill is already synced to public/skill.md.",
    );
    process.exit(0);
  }

  await mkdir(dirname(destinationPath), { recursive: true });
  await writeFile(destinationPath, source);
  console.log(
    "Synced skills/agentics-introduction/SKILL.md to public/skill.md.",
  );
} catch (error) {
  console.error(
    `Failed to sync Agentics introduction skill: ${error instanceof Error ? error.message : String(error)}`,
  );
  process.exit(1);
}
