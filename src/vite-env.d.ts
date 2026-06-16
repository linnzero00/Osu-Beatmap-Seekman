/// <reference types="vite/client" />

type DownloadTask = {
  id: string;
  groupId: string;
  groupName: string;
  groupSource: string;
  groupDestination: string;
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
  collectionBeatmapIds: number[];
  status: "pending" | "queued" | "downloading" | "paused" | "failed" | "completed" | "cancelled" | "staged";
  error: string;
  createdAt: string;
  updatedAt: string;
};

type DownloadGroupProgress = {
  id: string;
  name: string;
  source: string;
  destination: string;
  totalTasks: number;
  completedTasks: number;
  completedBytes: number;
  createdAt: string;
  updatedAt: string;
};

type UpdateInfo = {
  version: string;
  name: string;
  body: string;
  htmlUrl: string;
  publishedAt: string;
  canInstallNow: boolean;
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
  collectionBeatmapIds: number[];
  sourceCollection: string;
  playcount: number;
  favouriteCount: number;
  existsLocal?: boolean;
};

type StableCollectionSummary = {
  name: string;
  beatmapCount: number;
  items: BeatmapsetItem[];
};

type ImportedPlaylist = {
  items: BeatmapsetItem[];
  exportedAt: string;
  sourceCollection: string;
  title: string;
  author: string;
  description: string;
};

type PlaylistLocalApplyResult = {
  appliedCount: number;
  appliedBeatmapsetCount: number;
  missingCount: number;
  missingItems: BeatmapsetItem[];
};

interface Window {
  osuDownloader: {
    getState: () => Promise<any>;
    saveSettings: (settings: Record<string, unknown>) => Promise<any>;
    selectSongsDir: () => Promise<string | null>;
    selectLazerDir: () => Promise<string | null>;
    selectStableOsuDir: () => Promise<string | null>;
    scanSongs: (songsDir?: string) => Promise<any>;
    scanLazer: (lazerDir?: string) => Promise<any>;
    scanStableCollections: (stableOsuDir?: string) => Promise<StableCollectionSummary[]>;
    exportCollectionPlaylist: (stableOsuDir: string | undefined, collectionName: string, selectedBeatmapIds?: number[], playlistTitle?: string, playlistAuthor?: string, playlistDescription?: string) => Promise<string>;
    exportBeatmapsetPlaylist: (items: BeatmapsetItem[], sourceCollection?: string, playlistTitle?: string, playlistAuthor?: string, playlistDescription?: string) => Promise<string>;
    importSeekmanPlaylist: () => Promise<ImportedPlaylist>;
    applyLocalPlaylistItemsToCollection: (stableOsuDir: string | undefined, collectionName: string, items: BeatmapsetItem[], commit?: boolean) => Promise<PlaylistLocalApplyResult>;
    searchBeatmapsets: (filters: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
    searchAlphaRecommendations: (request: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
    searchUserBestScores: (request: Record<string, unknown>) => Promise<BeatmapsetItem[]>;
    enqueueDownloads: (items: BeatmapsetItem[]) => Promise<DownloadTask[]>;
    startDownloads: () => Promise<{ ok: boolean }>;
    pauseDownloads: () => Promise<{ ok: boolean }>;
    clearCompleted: () => Promise<DownloadTask[]>;
    retryFailedDownloads: () => Promise<DownloadTask[]>;
    clearAllDownloads: () => Promise<DownloadTask[]>;
    deleteDownloadGroup: (groupId: string) => Promise<DownloadTask[]>;
    forceFinishDownloadGroup: (groupId: string) => Promise<DownloadTask[]>;
    openApiPage: () => Promise<{ ok: boolean }>;
    openExternalUrl: (url: string) => Promise<{ ok: boolean }>;
    checkForUpdates: () => Promise<UpdateInfo | null>;
    dismissUpdateVersion: (version: string) => Promise<any>;
    installUpdateNow: () => Promise<{ ok: boolean }>;
    onDownloadEvent: (callback: (payload: any) => void) => () => void;
  };
}
