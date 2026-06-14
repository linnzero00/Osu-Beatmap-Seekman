import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Api = {
  getState: () => Promise<any>;
  saveSettings: (settings: Record<string, unknown>) => Promise<any>;
  selectSongsDir: () => Promise<string | null>;
  selectLazerDir: () => Promise<string | null>;
  selectStableOsuDir: () => Promise<string | null>;
  scanSongs: (songsDir?: string) => Promise<any>;
  scanLazer: (lazerDir?: string) => Promise<any>;
  scanStableCollections: (stableOsuDir?: string) => Promise<StableCollectionSummary[]>;
  exportCollectionPlaylist: (stableOsuDir: string | undefined, collectionName: string) => Promise<string>;
  importSeekmanPlaylist: () => Promise<BeatmapsetItem[]>;
  searchBeatmapsets: (filters: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
  searchAlphaRecommendations: (request: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
  enqueueDownloads: (items: BeatmapsetItem[]) => Promise<DownloadTask[]>;
  startDownloads: () => Promise<{ ok: boolean }>;
  pauseDownloads: () => Promise<{ ok: boolean }>;
  clearCompleted: () => Promise<DownloadTask[]>;
  retryFailedDownloads: () => Promise<DownloadTask[]>;
  clearAllDownloads: () => Promise<DownloadTask[]>;
  deleteDownloadGroup: (groupId: string) => Promise<DownloadTask[]>;
  openApiPage: () => Promise<{ ok: boolean }>;
  checkForUpdates: () => Promise<UpdateInfo | null>;
  dismissUpdateVersion: (version: string) => Promise<any>;
  installUpdateNow: () => Promise<{ ok: boolean }>;
  onDownloadEvent: (callback: (payload: any) => void) => () => void;
};

const browserFallback: Api = {
  getState: async () => ({ settings: {}, tasks: [], localBeatmapsets: {} }),
  saveSettings: async (settings) => settings,
  selectSongsDir: async () => {
    alert("请用 `npm run dev` 打开 Tauri 桌面端；浏览器预览不能弹出本地 Songs 文件夹选择器。");
    return null;
  },
  selectLazerDir: async () => {
    alert("请用 Tauri 桌面端选择 osu! lazer 目录；浏览器预览不能弹出本地文件夹选择器。");
    return null;
  },
  selectStableOsuDir: async () => {
    alert("请用 Tauri 桌面端选择 osu!stable 根目录；浏览器预览不能弹出本地文件夹选择器。");
    return null;
  },
  scanSongs: async () => ({ count: 0, localBeatmapsets: {} }),
  scanLazer: async () => ({ count: 0, localBeatmapsets: {} }),
  scanStableCollections: async () => [],
  exportCollectionPlaylist: async () => "",
  importSeekmanPlaylist: async () => [],
  searchBeatmapsets: async () => [],
  searchAlphaRecommendations: async () => [],
  enqueueDownloads: async () => [],
  startDownloads: async () => ({ ok: false }),
  pauseDownloads: async () => ({ ok: false }),
  clearCompleted: async () => [],
  retryFailedDownloads: async () => [],
  clearAllDownloads: async () => [],
  deleteDownloadGroup: async () => [],
  openApiPage: async () => {
    window.open("https://osu.ppy.sh/home/account/edit#authenticator-app", "_blank", "noopener,noreferrer");
    return { ok: true };
  },
  checkForUpdates: async () => null,
  dismissUpdateVersion: async () => ({}),
  installUpdateNow: async () => ({ ok: false }),
  onDownloadEvent: () => () => undefined,
};

const electronApi = window.osuDownloader;
const isTauri = Boolean((window as any).__TAURI_INTERNALS__);

export const api: Api = electronApi ?? (isTauri ? {
  getState: () => invoke("get_state"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
  selectSongsDir: () => invoke("select_songs_dir"),
  selectLazerDir: () => invoke("select_lazer_dir"),
  selectStableOsuDir: () => invoke("select_stable_osu_dir"),
  scanSongs: (songsDir) => invoke("scan_songs", { songsDir }),
  scanLazer: (lazerDir) => invoke("scan_lazer", { lazerDir }),
  scanStableCollections: (stableOsuDir) => invoke("scan_stable_collections", { stableOsuDir }),
  exportCollectionPlaylist: (stableOsuDir, collectionName) => invoke("export_collection_playlist", { stableOsuDir, collectionName }),
  importSeekmanPlaylist: () => invoke("import_seekman_playlist"),
  searchBeatmapsets: (filters) => invoke("search_beatmapsets", { filters }),
  searchAlphaRecommendations: (request) => invoke("search_alpha_recommendations", { request }),
  enqueueDownloads: (items) => invoke("enqueue_downloads", { items }),
  startDownloads: () => invoke("start_downloads"),
  pauseDownloads: () => invoke("pause_downloads"),
  clearCompleted: () => invoke("clear_completed"),
  retryFailedDownloads: () => invoke("retry_failed_downloads"),
  clearAllDownloads: () => invoke("clear_all_downloads"),
  deleteDownloadGroup: (groupId) => invoke("delete_download_group", { groupId }),
  openApiPage: () => invoke("open_api_page"),
  checkForUpdates: () => invoke("check_for_updates"),
  dismissUpdateVersion: (version) => invoke("dismiss_update_version", { version }),
  installUpdateNow: () => invoke("install_update_now"),
  onDownloadEvent: (callback) => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen("downloads:event", (event) => callback(event.payload)).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  },
} : browserFallback);
