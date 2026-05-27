#!/usr/bin/env bun
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const sourceRoot = join(webRoot, "src");
const checkedExtensions = new Set([
  ".css",
  ".js",
  ".jsx",
  ".mjs",
  ".ts",
  ".tsx",
]);
const colorUtilities = [
  "accent",
  "bg",
  "border",
  "caret",
  "decoration",
  "divide",
  "fill",
  "from",
  "outline",
  "placeholder",
  "ring",
  "shadow",
  "stroke",
  "text",
  "to",
  "via",
];
const paletteNames = [
  "amber",
  "blue",
  "cyan",
  "emerald",
  "fuchsia",
  "gray",
  "green",
  "indigo",
  "lime",
  "neutral",
  "orange",
  "pink",
  "purple",
  "red",
  "rose",
  "sky",
  "slate",
  "stone",
  "teal",
  "violet",
  "yellow",
  "zinc",
];
const genericShapeUtilities = [
  ...["sm", "md", "lg", "xl", "2xl", "3xl", "4xl"].map(
    (size) => `rounded-${size}`,
  ),
  ...["sm", "md", "lg", "xl", "2xl"].map((size) => `shadow-${size}`),
];
const classBoundary = String.raw`[\s"'\`]`;
const classStart = `(?:^|${classBoundary})`;
const classEnd = `(?=$|${classBoundary})`;

const utilityPattern = new RegExp(
  String.raw`${classStart}((?:[a-z-]+:)*(?:${colorUtilities.join("|")})-(?:${paletteNames.join("|")})-\d{2,3}(?:\/\d{1,3})?)`,
  "g",
);
const darkColorPattern = new RegExp(
  String.raw`${classStart}((?:[a-z-]+:)*dark:(?:[a-z-]+:)*(?:${colorUtilities.join("|")})-[^\s"'\`]+)`,
  "g",
);
const genericShapePattern = new RegExp(
  `${classStart}((?:[a-z-]+:)*(?:${genericShapeUtilities.join("|")}))${classEnd}`,
  "g",
);
const ambiguousVisTypePattern = new RegExp(
  String.raw`${classStart}((?:[a-z-]+:)*(?:font-\[var\(--font-(?:sans|mono)\)\]|text-\[var\(--text-(?:hero|h1|h2|h3|body|body-sm|caption|mono)\)\]|leading-\[var\(--leading-(?:hero|h1|h2|h3|body|body-sm|caption|mono)\)\]))`,
  "g",
);

function listFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      return listFiles(path);
    }
    if (!entry.isFile()) {
      return [];
    }
    const extension = path.slice(path.lastIndexOf("."));
    return checkedExtensions.has(extension) ? [path] : [];
  });
}

function lineAndColumn(source, index) {
  const beforeMatch = source.slice(0, index);
  const lines = beforeMatch.split("\n");
  return {
    line: lines.length,
    column: lines.at(-1).length + 1,
  };
}

const violations = [];

for (const file of listFiles(sourceRoot)) {
  const source = readFileSync(file, "utf8");
  for (const pattern of [
    utilityPattern,
    darkColorPattern,
    genericShapePattern,
    ambiguousVisTypePattern,
  ]) {
    pattern.lastIndex = 0;
    for (const match of source.matchAll(pattern)) {
      const token = match[1];
      const tokenStart = match.index + match[0].lastIndexOf(token);
      const location = lineAndColumn(source, tokenStart);
      violations.push({
        file: relative(webRoot, file),
        token,
        ...location,
      });
    }
  }
}

if (violations.length > 0) {
  console.error(
    "VIS/Tailwind guardrail failed. Use VIS semantic utilities or CSS variables instead of raw Tailwind brand colors, generic shape shadows, or ambiguous VIS type arbitrary values:",
  );
  for (const violation of violations) {
    console.error(
      `  ${violation.file}:${violation.line}:${violation.column} ${violation.token}`,
    );
  }
  process.exit(1);
}

console.log("VIS/Tailwind guardrail passed.");
