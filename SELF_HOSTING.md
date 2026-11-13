# Self Hosting Guide

## Prerequisites

- [Docker & Docker Compose](https://docs.docker.com/compose/install/) for the containerized setup.

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
- `ADMIN_PASSWORD`: (Optional) Initial admin dashboard password to seed on first startup instead of generating a random one.
