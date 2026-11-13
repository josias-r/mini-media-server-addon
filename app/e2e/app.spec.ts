import { expect, test } from "@playwright/test";

test.describe.serial("Mini Media Server E2E", () => {
  let aliceApiKey = "";

  test("User Portal loads and validates invalid API keys", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByText("Mini Media Server", { exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Connect Your Media Client" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Stremio", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Fusion — Media Center" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Harbor" })).toBeVisible();

    const keyInput = page.locator("#api-key-input");
    await keyInput.fill("non-existent-key-123");

    await expect(page.getByText("Invalid API Key")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Enter Valid Key for 1-Click Install" }),
    ).toBeDisabled();
  });

  test("Admin authentication, user creation, reveal key, and Stremio manifest", async ({
    page,
    request,
  }) => {
    await page.goto("/admin");

    await expect(page.getByRole("heading", { name: "Admin Control Center" })).toBeVisible();

    // Wrong password shows error
    await page.locator("#admin-password").fill("wrong-password");
    await page.getByRole("button", { name: "Sign In" }).click();
    await expect(page.getByText("Invalid password")).toBeVisible();

    // Valid password logs into dashboard
    await page.locator("#admin-password").fill("test-admin-pass");
    await page.getByRole("button", { name: "Sign In" }).click();

    await expect(page.getByRole("heading", { name: "Users & Invite Links" })).toBeVisible();
    await expect(page.getByText("7 Days")).toBeVisible();

    // Create user alice
    await page.getByPlaceholder("New username...").fill("alice");
    await page.getByRole("button", { name: "Add User" }).click();

    await expect(page.getByText("User “alice” created!")).toBeVisible();

    const aliceRow = page.locator("tbody tr", { hasText: "alice" });
    await expect(aliceRow).toBeVisible();

    // Key is masked by default, reveal button exposes full UUID
    await expect(aliceRow.locator("code")).toContainText("••••••••••••");
    await aliceRow.getByTitle("Reveal key").click();
    aliceApiKey = (await aliceRow.locator("code").innerText()).trim();
    expect(aliceApiKey).toMatch(/^[0-9a-f-]{36}$/i);

    await aliceRow.getByTitle("Hide key").click();
    await expect(aliceRow.locator("code")).toContainText("••••••••••••");

    // Verify real Stremio manifest endpoint works for alice's API key
    const manifestRes = await request.get(`/${aliceApiKey}/manifest.json`);
    expect(manifestRes.status()).toBe(200);
    const manifestJson = (await manifestRes.json()) as {
      id: string;
      name: string;
      resources: string[];
    };
    expect(manifestJson.id).toBe("mini-media-server.addon");
    expect(manifestJson.name).toBe("Mini Media Server");
    expect(manifestJson.resources).toContain("stream");
  });

  test("Invite link auto-configures User Portal and persists to localStorage", async ({ page }) => {
    expect(aliceApiKey).not.toBe("");

    await page.goto(`/?key=${encodeURIComponent(aliceApiKey)}`);

    // Query param is stripped from URL and stored in localStorage
    await expect(page).toHaveURL("http://localhost:7099/");
    await expect(page.getByText("Verified • Welcome, alice")).toBeVisible();
    await expect(page.locator("code", { hasText: `/${aliceApiKey}/manifest.json` })).toBeVisible();

    const installLink = page.getByRole("link", { name: "1-Click Install in Stremio" });
    await expect(installLink).toHaveAttribute(
      "href",
      `stremio://localhost:7099/${aliceApiKey}/manifest.json`,
    );

    await expect(
      page.getByRole("heading", { name: "My Active Downloads & Recent Streams" }),
    ).toBeVisible();

    // Reloading preserves verified session via localStorage
    await page.reload();
    await expect(page.getByText("Verified • Welcome, alice")).toBeVisible();
  });

  test("User Portal and Admin Dashboard torrent rendering, filtering, and deletion", async ({
    page,
  }) => {
    const mockTorrents = [
      {
        hash: "aaaa1111bbbb2222cccc3333dddd4444eeee5555",
        name: "Big.Buck.Bunny.2008.1080p.mkv",
        size: 2_147_483_648,
        progress: 0.45,
        state: "downloading",
        dlspeed: 5_242_880,
        upspeed: 0,
        eta: 180,
        added_on: Math.floor(Date.now() / 1000) - 300,
        last_watched_at: "2026-11-13 20:00:00",
        expires_at: "2026-11-20T20:00:00Z",
      },
      {
        hash: "ffff9999eeee8888dddd7777cccc6666bbbb5555",
        name: "Sintel.2010.2160p.WEB-DL.mkv",
        size: 4_294_967_296,
        progress: 1.0,
        state: "uploading",
        dlspeed: 0,
        upspeed: 1_048_576,
        eta: 0,
        added_on: Math.floor(Date.now() / 1000) - 3600,
        last_watched_at: null,
        expires_at: "2026-11-20T19:00:00Z",
      },
    ];

    let torrentsState = [...mockTorrents];

    await page.route("**/api/verify*", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          valid: true,
          username: "alice",
          created_at: "2026-11-13 20:00:00",
          active_torrents: [
            {
              hash: torrentsState[0].hash,
              name: torrentsState[0].name,
              size: torrentsState[0].size,
              progress: torrentsState[0].progress,
              state: torrentsState[0].state,
              dlspeed: torrentsState[0].dlspeed,
              last_watched_at: torrentsState[0].last_watched_at,
            },
          ],
        }),
      });
    });

    await page.goto(`/?key=${encodeURIComponent(aliceApiKey)}`);
    await expect(page.getByText("Big.Buck.Bunny.2008.1080p.mkv")).toBeVisible();
    await expect(page.getByText("Downloading • 45%")).toBeVisible();

    await page.unroute("**/api/verify*");

    await page.route("**/api/admin/overview", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          health: { qbittorrent: true, jackett: true },
          config: {
            public_url: "http://localhost:7099",
            retention_days: 7,
            jackett_search_type: "text",
          },
          users: [
            {
              username: "alice",
              api_key: aliceApiKey,
              created_at: "2026-11-13 20:00:00",
              watch_count: 1,
              last_watched_at: "2026-11-13 20:00:00",
            },
          ],
          torrents: torrentsState,
        }),
      });
    });

    await page.route("**/api/admin/torrents/*", async (route) => {
      const url = route.request().url();
      const hash = url.split("/").pop() ?? "";
      torrentsState = torrentsState.filter((t) => t.hash !== hash);
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ok: true }),
      });
    });

    await page.goto("/admin");
    await page.locator("#admin-password").fill("test-admin-pass");
    await page.getByRole("button", { name: "Sign In" }).click();

    await expect(page.getByText("Big.Buck.Bunny.2008.1080p.mkv")).toBeVisible();
    await expect(page.getByText("Sintel.2010.2160p.WEB-DL.mkv")).toBeVisible();

    // Filter tabs
    await page.getByRole("button", { name: "Downloading", exact: true }).click();
    await expect(page.getByText("Big.Buck.Bunny.2008.1080p.mkv")).toBeVisible();
    await expect(page.getByText("Sintel.2010.2160p.WEB-DL.mkv")).toBeHidden();

    await page.getByRole("button", { name: "Never Watched", exact: true }).click();
    await expect(page.getByText("Sintel.2010.2160p.WEB-DL.mkv")).toBeVisible();
    await expect(page.getByText("Big.Buck.Bunny.2008.1080p.mkv")).toBeHidden();

    await page.getByRole("button", { name: "All", exact: true }).click();

    // Search filter
    await page.getByPlaceholder("Filter by torrent name or hash...").fill("sintel");
    await expect(page.getByText("Sintel.2010.2160p.WEB-DL.mkv")).toBeVisible();
    await expect(page.getByText("Big.Buck.Bunny.2008.1080p.mkv")).toBeHidden();
    await page.getByPlaceholder("Filter by torrent name or hash...").fill("");

    // 2-step delete torrent
    const sintelCard = page
      .locator("div.rounded-2xl", { hasText: "Sintel.2010.2160p.WEB-DL.mkv" })
      .first();
    await sintelCard.getByRole("button", { name: "Delete" }).click();
    await sintelCard.getByRole("button", { name: "Confirm Delete" }).click();
    await expect(page.getByText("Sintel.2010.2160p.WEB-DL.mkv")).toBeHidden();

    await page.unroute("**/api/admin/overview");
    await page.unroute("**/api/admin/torrents/*");
  });

  test("Admin password change, re-login, user deletion, and logout", async ({ page, request }) => {
    await page.goto("/admin");
    await page.locator("#admin-password").fill("test-admin-pass");
    await page.getByRole("button", { name: "Sign In" }).click();
    await expect(page.getByRole("heading", { name: "Users & Invite Links" })).toBeVisible();

    // Change password
    await page.getByRole("button", { name: "Password" }).click();
    const dialog = page.getByRole("dialog", { name: "Change Admin Password" });
    await expect(dialog).toBeVisible();

    await dialog.locator("#new-admin-password").fill("new-secret-pass");
    await dialog.locator("#confirm-admin-password").fill("new-secret-pass");
    await dialog.getByRole("button", { name: "Save Password" }).click();
    await expect(dialog.getByText("Admin password updated successfully.")).toBeVisible();
    await dialog.getByRole("button", { name: "Close", exact: true }).click();

    // Logout & verify old password fails, new password succeeds
    await page.getByRole("button", { name: "Logout" }).click();
    await expect(page.getByRole("heading", { name: "Admin Control Center" })).toBeVisible();

    await page.locator("#admin-password").fill("test-admin-pass");
    await page.getByRole("button", { name: "Sign In" }).click();
    await expect(page.getByText("Invalid password")).toBeVisible();

    await page.locator("#admin-password").fill("new-secret-pass");
    await page.getByRole("button", { name: "Sign In" }).click();
    await expect(page.getByRole("heading", { name: "Users & Invite Links" })).toBeVisible();

    // 2-step delete alice
    const aliceRow = page.locator("tbody tr", { hasText: "alice" });
    await aliceRow.getByTitle("Revoke user").click();
    await aliceRow.getByRole("button", { name: "Confirm" }).click();

    await expect(page.locator("tbody tr", { hasText: "alice" })).toHaveCount(0);

    // Verify API key is now invalid on backend
    const verifyRes = await request.get(`/api/verify?api_key=${encodeURIComponent(aliceApiKey)}`);
    expect(verifyRes.status()).toBe(200);
    const verifyJson = (await verifyRes.json()) as { valid: boolean };
    expect(verifyJson.valid).toBe(false);
  });
});
