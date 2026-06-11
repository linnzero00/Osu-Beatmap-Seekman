/// <reference types="vite/client" />

type DownloadTask = {
  id: string;
  beatmapsetId: number;
  title: string;
  artist: string;
  includeVideo: boolean;
  downloadMode: "video" | "noVideo" | "osu";
  beatmapId: number | null;
  url: string;
  targetPath: string;
  tempPath: string;
  totalBytes: number | null;
  downloadedBytes: number;
  retryGeneration: number;
  status: "pending" | "queued" | "downloading" | "paused" | "failed" | "completed" | "cancelled";
  error: string;
  createdAt: string;
  updatedAt: string;
};

type BeatmapsetItem = {
  id: number;
  title: string;
  artist: string;
  creator: string;
  rankedDate: string;
  status: string;
  modes: string[];
  minStars: number | null;
  maxStars: number | null;
  minOd: number | null;
  maxOd: number | null;
  minHp: number | null;
  maxHp: number | null;
  minCs: number | null;
  maxCs: number | null;
  minAr: number | null;
  maxAr: number | null;
  minBpm: number | null;
  maxBpm: number | null;
  minLength: number | null;
  maxLength: number | null;
  keyCounts: number[];
  beatmapIds: number[];
  playcount: number;
  favouriteCount: number;
  existsLocal?: boolean;
};

interface Window {
  osuDownloader: {
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
    openApiPage: () => Promise<{ ok: boolean }>;
    onDownloadEvent: (callback: (payload: any) => void) => () => void;
  };
}
