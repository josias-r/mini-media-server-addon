import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:7099",
    trace: "on-first-retry",
  },
  webServer: {
    command:
      "pnpm run build && (cd .. && cargo build) && rm -rf /tmp/mms-e2e-data && mkdir -p /tmp/mms-e2e-data && DATA_DIR=/tmp/mms-e2e-data STATIC_DIR=dist PORT=7099 PUBLIC_URL=http://localhost:7099 ADMIN_PASSWORD=test-admin-pass JACKETT_URL=http://127.0.0.1:59991 JACKETT_API_KEY=test-key JACKETT_SEARCH_TYPE=text QBITTORRENT_URL=http://127.0.0.1:59992 QBITTORRENT_USERNAME=admin QBITTORRENT_PASSWORD=admin DOWNLOAD_PATH=/tmp/mms-e2e-data/downloads RETENTION_DAYS=7 ../target/debug/mini-media-server-addon",
    url: "http://localhost:7099/api/info",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
