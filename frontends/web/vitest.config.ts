import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const rootDir = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    env: {
      AGENTICS_DEPLOYMENT_STAGE: "test",
      AGENTICS_API_BASE_URL: "http://127.0.0.1:3100",
      AGENTICS_WEB_PORT: "3001",
    },
  },
  resolve: {
    alias: {
      "@": resolve(rootDir, "src"),
    },
  },
});
