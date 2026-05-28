import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";

type RehearsalChallenge = {
  name: string;
  title: string;
  mode: string;
  target: string;
};

type RehearsalSubmissions = {
  separated_official_id?: string;
  piped_stdio_official_id?: string;
  coexecuted_official_id?: string;
};

type RehearsalManifest = {
  run_id: string;
  web_base_url: string;
  challenges: RehearsalChallenge[];
  submissions: RehearsalSubmissions;
};

function readManifest(): RehearsalManifest {
  const path = process.env.AGENTICS_REHEARSAL_MANIFEST;
  if (!path) {
    throw new Error(
      "AGENTICS_REHEARSAL_MANIFEST must point at browser-manifest.json",
    );
  }
  return JSON.parse(readFileSync(path, "utf8")) as RehearsalManifest;
}

function pageUrl(manifest: RehearsalManifest, path: string): string {
  return new URL(path, manifest.web_base_url).toString();
}

function exactText(value: string): RegExp {
  return new RegExp(`^${value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
}

test.describe("production rehearsal observer surfaces", () => {
  const manifest = readManifest();

  test("catalog and challenge detail pages load seeded rehearsal fixtures", async ({
    page,
  }) => {
    await page.goto(pageUrl(manifest, "/"));
    await expect(page.locator("body")).toContainText("Agentics");

    await page.goto(pageUrl(manifest, "/challenges"));
    for (const challenge of manifest.challenges) {
      await expect(page.locator("body")).toContainText(challenge.title);
    }

    for (const challenge of manifest.challenges) {
      await page.goto(
        pageUrl(manifest, `/challenges/${encodeURIComponent(challenge.name)}`),
      );
      await expect(
        page.getByRole("heading", { name: exactText(challenge.title) }).first(),
      ).toBeVisible();
      await expect(page.locator("body")).not.toContainText("private-benchmark");
    }
  });

  test("leaderboard and submission detail pages expose public results only", async ({
    page,
  }) => {
    const separated = manifest.challenges.find(
      (challenge) => challenge.mode === "separated_evaluator",
    );
    const separatedOfficialId = manifest.submissions.separated_official_id;
    if (!separated || !separatedOfficialId) {
      test.skip(true, "separated official rehearsal submission is unavailable");
      return;
    }

    await page.goto(
      pageUrl(
        manifest,
        `/challenges/${encodeURIComponent(separated.name)}/leaderboard?target=${encodeURIComponent(separated.target)}`,
      ),
    );
    await expect(page.locator("body")).toContainText(separated.title);
    await expect(page.locator("body")).toContainText(
      separatedOfficialId.slice(0, 8),
    );

    await page.goto(
      pageUrl(
        manifest,
        `/solution-submissions/${encodeURIComponent(separatedOfficialId)}`,
      ),
    );
    await expect(page.locator("body")).toContainText(separated.title);
    await expect(page.locator("body")).toContainText(
      separatedOfficialId.slice(0, 8),
    );
    await expect(page.locator("body")).not.toContainText(
      "validation_evaluation",
    );
    await expect(page.locator("body")).not.toContainText("private-benchmark");
  });
});
