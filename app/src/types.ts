export interface PublicInfo {
  public_url: string;
  retention_days: number;
  jackett_search_type: string;
  version: string;
}

export interface UserTorrentStatus {
  hash: string;
  name: string;
  size: number;
  progress: number;
  state: string;
  dlspeed: number;
  last_watched_at: string;
}

export interface VerifyKeyResponse {
  valid: boolean;
  username?: string;
  created_at?: string;
  active_torrents: UserTorrentStatus[];
}

export interface AdminHealth {
  qbittorrent: boolean;
  jackett: boolean;
}

export interface AdminConfigInfo {
  public_url: string;
  retention_days: number;
  jackett_search_type: string;
}

export interface AdminUser {
  username: string;
  api_key: string;
  created_at: string;
  watch_count: number;
  last_watched_at: string | null;
}

export interface AdminTorrent {
  hash: string;
  name: string;
  size: number;
  progress: number;
  state: string;
  dlspeed: number;
  upspeed: number;
  eta: number;
  added_on: number;
  last_watched_at: string | null;
  expires_at: string | null;
}

export interface AdminOverview {
  health: AdminHealth;
  config: AdminConfigInfo;
  users: AdminUser[];
  torrents: AdminTorrent[];
}

export interface CreatedUserResponse {
  username: string;
  api_key: string;
  created_at: string;
}

export type OnboardingStage = "install" | "watch" | "done";

export type TorrentFilter = "all" | "downloading" | "seeding" | "unwatched";

export type TorrentSort =
  | "added_desc"
  | "added_asc"
  | "size_desc"
  | "watched_desc"
  | "progress_desc";

export interface AdminStats {
  userCount: number;
  totalStreams: number;
  totalBytes: number;
  torrentCount: number;
  dlSpeed: number;
  upSpeed: number;
  downloadingCount: number;
}
