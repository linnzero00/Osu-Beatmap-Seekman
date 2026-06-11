import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Api = {
  getState: () => Promise<any>;
  saveSettings: (settings: Record<string, unknown>) => Promise<any>;
  selectSongsDir: () => Promise<string | null>;
  scanSongs: (songsDir?: string) => Promise<any>;
  searchBeatmapsets: (filters: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
  enqueueDownloads: (items: BeatmapsetItem[]) => Promise<DownloadTask[]>;
  startDownloads: () => Promise<{ ok: boolean }>;
  pauseDownloads: () => Promise<{ ok: boolean }>;
  clearCompleted: () => Promise<DownloadTask[]>;
  retryFailedDownloads: () => Promise<DownloadTask[]>;
  clearAllDownloads: () => Promise<DownloadTask[]>;
  onDownloadEvent: (callback: (payload: any) => void) => () => void;
};

const browserFallback: Api = {
  getState: async () => ({ settings: {}, tasks: [], localBeatmapsets: {} }),
  saveSettings: async (settings) => settings,
  selectSongsDir: async () => {
    alert("请用 `npm run dev` 打开 Tauri 桌面端；浏览器预览不能弹出本地 Songs 文件夹选择器。");
    return null;
  },
  scanSongs: async () => ({ count: 0, localBeatmapsets: {} }),
  searchBeatmapsets: async () => [],
  enqueueDownloads: async () => [],
  startDownloads: async () => ({ ok: false }),
  pauseDownloads: async () => ({ ok: false }),
  clearCompleted: async () => [],
  retryFailedDownloads: async () => [],
  clearAllDownloads: async () => [],
  onDownloadEvent: () => () => undefined,
};

const electronApi = window.osuDownloader;
const isTauri = Boolean((window as any).__TAURI_INTERNALS__);

export const api: Api = electronApi ?? (isTauri ? {
  getState: () => invoke("get_state"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
  selectSongsDir: () => invoke("select_songs_dir"),
  scanSongs: (songsDir) => invoke("scan_songs", { songsDir }),
  searchBeatmapsets: (filters) => invoke("search_beatmapsets", { filters }),
  enqueueDownloads: (items) => invoke("enqueue_downloads", { items }),
  startDownloads: () => invoke("start_downloads"),
  pauseDownloads: () => invoke("pause_downloads"),
  clearCompleted: () => invoke("clear_completed"),
  retryFailedDownloads: () => invoke("retry_failed_downloads"),
  clearAllDownloads: () => invoke("clear_all_downloads"),
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
