# Self Hosting Guide

## Prerequisites

- [Docker & Docker Compose](https://docs.docker.com/compose/install/) if you intend to use the dockerized setup - for self hosting.
- Alternatively, you can run the addon natively with `cargo run` (requires [Rust](https://rustup.rs/)). - for development purposes.

## Using Docker Compose

The easiest way to get started is with the included `docker-compose.yml` file.

1. **Configure Environment:** Edit the `docker-compose.yml` and modify the environment variables block under `x-config`, specifically setting your `public_url`, and the API credentials for qBittorrent and Jackett.
2. **Start Services:**
   ```bash
   docker-compose up -d
   ```
3. **Initial Setup:**
   - Access **Jackett** at `http://<your-ip>:9117` and set up your indexers. Copy your API Key to the docker configuration or `.env` file.
   - Check the **qBittorrent** logs (`docker-compose logs qbittorrent`) to find the generated initial admin password.
   - Access **qBittorrent** at `http://<your-ip>:8080` using the username `admin` and the password from the logs, then set a new password in the settings. Update your docker configuration or `.env` file with this new password.
4. **Restart the Addon Server** so it picks up the correct API credentials.
5. **Admin Panel & API Keys:**
   - Check the Mini Media Server addon logs (`docker-compose logs addon`) to find the generated `admin` password.
   - Access the Admin Panel at `http://<your-ip>:<port>/admin` and log in.
   - Generate a user API key.
6. **Access the Addon:**
   Navigate to the URL defined as your `public_url` in your web browser. This page will prompt you to enter your API key to generate your Stremio manifest URL.

## Environment Variables

Here is a list of important environment variables you can configure (either via `docker-compose.yml` or a `.env` file):

- `PORT`: The port the server listens on (default: `3000`).
- `PUBLIC_URL`: The publicly accessible URL where this addon is hosted.
- `JACKETT_URL`: URL to your Jackett instance (usually docker container url, e.g., `http://jackett:9117`).
- `JACKETT_API_KEY`: API Key for Jackett.
- `JACKETT_SEARCH_TYPE`: Either `text` or `imdb`. `text` is more flexible but less accurate.
- `QBITTORRENT_URL`: URL to your qBittorrent instance (usually docker container url, e.g., `http://qbittorrent:8080`).
- `QBITTORRENT_USERNAME`: Username for qBittorrent WebUI.
- `QBITTORRENT_PASSWORD`: Password for qBittorrent WebUI.
- `DOWNLOAD_PATH`: The directory path inside the addon container where downloaded torrents are located.
- `RETENTION_DAYS`: (Optional) Number of days to keep a downloaded torrent before it is automatically deleted. `0` disables this feature (default: `0`).

## Running with Cargo (Local Development)

> [!NOTE]
> Running the project via Cargo **only runs the addon server**; it does not start the Jackett and qBittorrent services.
> Furthermore, actual file-based streaming will not work because the addon must share a Docker volume (or local path) with qBittorrent to access the downloaded files. This section is primarily intended for local development, pointing to existing external servers.

If you prefer to run the project via Cargo for development purposes:

1. Create a `.env` file in the project root directory with the following variables:
   ```env
   JACKETT_URL=http://localhost:9117
   JACKETT_API_KEY=your_jackett_api_key
   PUBLIC_URL=http://localhost:7000
   QBITTORRENT_URL=http://localhost:8080
   QBITTORRENT_USERNAME=admin
   QBITTORRENT_PASSWORD=your_qbittorrent_password
   DOWNLOAD_PATH=/path/to/downloads
   JACKETT_SEARCH_TYPE=text
   PORT=7000
   RETENTION_DAYS=0
   ```
2. Build and run the project:
   ```bash
   cargo run --release
   ```
3. The addon homepage will be available at `http://localhost:7000`.
