# Mini Media Server Addon

The quickest, most seamless way to deploy your own self-hosted media server. This Stremio-compatible addon seamlessly integrates Jackett and qBittorrent to provide a blazing-fast, debrid-like streaming experience directly to your favorite clients.

## 🌟 Features

- **Universal Indexer Support (Jackett):** Effortlessly connect to Jackett and configure any indexer or tracker you prefer for maximum content availability.
- **Smart Torrent Matching:** Automatically scans available torrents, finds the highest quality releases, and intelligently matches them to your requested movies or TV shows.
- **Direct On-the-Fly Streaming:** No need to wait for full downloads. The server instantly initiates torrents and seamlessly streams the media files directly to your client using HTTP range requests.
- **Secure Access Control:** Protect your server using generated API keys. Control exactly who has access to your media and prevent unauthorized usage.
- **Admin Dashboard:** A lightweight, built-in admin panel to manage users, track downloaded files, and monitor viewing history in real-time.
- **Automated Storage Management:** Intelligently tracks viewing habits, allowing you to configure automatic deletion of media files that haven't been watched recently to save disk space.
- **Broad Client Compatibility:** Works flawlessly with any Stremio-compatible client, including the official Stremio apps, Fusion Media Center, Harbor, and more.

## Getting Started

Ready to self-host your own debrid-like media server?

👉 [**Check out the Self Hosting Guide here**](SELF_HOSTING.md)

## Local Development

```bash
# 1. Backend (http://localhost:3000)
cp .env.example .env
cargo run

# 2. Frontend (http://localhost:5173, proxies /api -> :3000)
cd app && corepack enable pnpm && pnpm install && pnpm dev
```

For checks and Playwright E2E tests (starts its own test server):

```bash
cd app && pnpm format:check && pnpm lint && pnpm typecheck && pnpm test:e2e
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
```
