import { CalendarDays, Download, FolderOpen, Gauge, HelpCircle, Palette, Pause, Play, RotateCcw, Search, Settings } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";

type Mode = "any" | "osu" | "taiko" | "fruits" | "mania";
type LocalSource = "stable" | "lazer";
type AppTab = "settings" | "search" | "downloads" | "playlists";

const defaultMirrorPriority = ["hinamizawa", "catboy", "nerinyan", "sayobot"];
const themeOptions = [
  { id: "lime", label: "BFFF00 + 222222", primary: "#BFFF00", surface: "#222222" },
  { id: "cyan", label: "2C2C34 + 00D4FF", primary: "#00D4FF", surface: "#2C2C34" },
  { id: "sky", label: "89C2FF + E6E7FF", primary: "#89C2FF", surface: "#E6E7FF" },
];
const mirrorLabels: Record<string, string> = {
  hinamizawa: "Hinamizawa",
  catboy: "Catboy",
  nerinyan: "Nerinyan",
  sayobot: "Sayobot",
};

const defaultFilters = {
  query: "", status: "ranked", dateFrom: "", dateTo: "", minStars: "3", maxStars: "7",
  minOd: "0", maxOd: "10", minHp: "0", maxHp: "10", minCs: "0", maxCs: "10",
  minAr: "0", maxAr: "10", minBpm: "0", maxBpm: "400", minLength: "", maxLength: "",
  mode: "osu" as Mode, keyCount: "any", maxPages: "50", sortBy: "time", sortDir: "desc",
};
const defaultAlpha = { username: "", limit: "100", mode: "mania" as Mode, keyCount: "4" };
const defaultBest = { username: "", limit: "100", mode: "mania" as Mode };

export function App() {
  const [settings, setSettings] = useState({
    songsDir: "", lazerDir: "", stableOsuDir: "", osuClientId: "", osuClientSecret: "", bearerToken: "", concurrentDownloads: 8,
    includeVideo: true, downloadMode: "video", hideExisting: false, collectionAutoAdd: false, collectionName: "Seekman Downloads", localSource: "stable" as LocalSource, mirrorPriority: defaultMirrorPriority, mixedMode: false, theme: "cyan",
  });
  const [filters, setFilters] = useState(defaultFilters);
  const [alpha, setAlpha] = useState(defaultAlpha);
  const [best, setBest] = useState(defaultBest);
  const [lastBestMode, setLastBestMode] = useState<Mode | null>(null);
  const [items, setItems] = useState<BeatmapsetItem[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [taskGroupProgress, setTaskGroupProgress] = useState<Record<string, DownloadGroupProgress>>({});
  const [localBeatmapsets, setLocalBeatmapsets] = useState<Record<string, { detectedFrom?: string }>>({});
  const [localCount, setLocalCount] = useState(0);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [confirmDeleteGroup, setConfirmDeleteGroup] = useState<string | null>(null);
  const [confirmForceGroup, setConfirmForceGroup] = useState<string | null>(null);
  const [exportPlaylistOpen, setExportPlaylistOpen] = useState(false);
  const [playlistPathsHelpOpen, setPlaylistPathsHelpOpen] = useState(false);
  const [collectionRiskOpen, setCollectionRiskOpen] = useState(false);
  const [searchHelpOpen, setSearchHelpOpen] = useState(false);
  const [apiHelpOpen, setApiHelpOpen] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const [stableCollections, setStableCollections] = useState<StableCollectionSummary[]>([]);
  const [activeTab, setActiveTab] = useState<AppTab>("search");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [collectionTargetMode, setCollectionTargetMode] = useState<"existing" | "new">("existing");
  const [playlistSource, setPlaylistSource] = useState<"collection" | "import" | "search">("search");
  const [playlistMeta, setPlaylistMeta] = useState<{ exportedAt: string; sourceCollection: string; title: string; author: string; description: string } | null>(null);
  const [playlistExportDraft, setPlaylistExportDraft] = useState({ title: "", author: "", description: "" });
  const [playlistExportContext, setPlaylistExportContext] = useState<{ mode: "collection" | "search"; sourceCollection: string }>({ mode: "collection", sourceCollection: "" });
  const [searchExportInfo, setSearchExportInfo] = useState({ title: "搜索", sourceCollection: "搜索" });
  const [pendingPlaylistImport, setPendingPlaylistImport] = useState<PlaylistLocalApplyResult | null>(null);
  const [pendingPlaylistAction, setPendingPlaylistAction] = useState<{ preview: PlaylistLocalApplyResult; items: BeatmapsetItem[] } | null>(null);

  useEffect(() => {
    api.getState().then((state) => {
      const nextSettings = normalizeSettings({ ...settings, ...state.settings });
      const nextLocal = state.localBeatmapsets || {};
      setSettings(nextSettings);
      setTasks(state.tasks || []);
      setTaskGroupProgress(state.taskGroups || {});
      setLocalBeatmapsets(nextLocal);
      setLocalCount(countLocalBySource(nextLocal, nextSettings.localSource));
    }).catch((error) => setMessage(String(error)));
    return api.onDownloadEvent((event) => {
      const type = event.kind ?? event.type;
      if (event.taskGroups) setTaskGroupProgress({ ...event.taskGroups });
      if (type === "tasks") setTasks([...(event.tasks || [])]);
      if (type === "progress" && event.task) {
        if (event.tasks) setTasks([...(event.tasks || [])]);
        else setTasks((current) => upsertTask(current, event.task));
      }
      if (type === "error") setMessage(event.error);
    });
  }, []);

  useEffect(() => {
    api.checkForUpdates().then((info) => {
      if (info) setUpdateInfo(info);
    }).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!tasks.length) return;
    const timer = window.setInterval(() => api.getState().then((state) => {
      setTasks(state.tasks || []);
      setTaskGroupProgress(state.taskGroups || {});
    }).catch(() => undefined), 500);
    return () => window.clearInterval(timer);
  }, [tasks.length]);

  useEffect(() => {
    document.documentElement.dataset.theme = normalizeTheme(settings.theme);
  }, [settings.theme]);

  const availableItems = useMemo(() => items.filter((item) => !item.existsLocal), [items]);
  const visibleItems = settings.hideExisting ? availableItems : items;
  const selectedItems = useMemo(() => availableItems.filter((item) => selectedIds.has(item.id)), [availableItems, selectedIds]);
  const playlistSelectableItems = useMemo(() => playlistSource === "search" ? availableItems : items, [playlistSource, items, availableItems]);
  const selectedPlaylistItems = useMemo(() => playlistSelectableItems.filter((item) => selectedIds.has(item.id)), [playlistSelectableItems, selectedIds]);
  const playlistVisibleItems = useMemo(() => {
    const list = [...items];
    list.sort((a, b) => Number(Boolean(a.existsLocal)) - Number(Boolean(b.existsLocal)) || a.artist.localeCompare(b.artist) || a.title.localeCompare(b.title));
    return list;
  }, [items]);
  const selectedDownloaded = tasks.reduce((sum, task) => sum + task.downloadedBytes, 0);
  const overall = getOverallProgress(tasks, taskGroupProgress);
  const taskGroups = useMemo(() => groupDownloadTasks(tasks, taskGroupProgress), [tasks, taskGroupProgress]);

  async function saveSettings(patch = settings) {
    const saved = await api.saveSettings(patch);
    setSettings((prev) => normalizeSettings({ ...prev, ...saved }));
  }

  async function selectSongsDir() {
    const dir = await api.selectSongsDir();
    if (dir) { setSettings((prev) => ({ ...prev, songsDir: dir })); setMessage(`已选择下载目录：${dir}`); }
  }

  async function selectLazerDir() {
    const dir = await api.selectLazerDir();
    if (dir) { setSettings((prev) => ({ ...prev, lazerDir: dir })); setMessage(`已选择 lazer 目录：${dir}`); }
  }

  async function selectStableOsuDir() {
    const dir = await api.selectStableOsuDir();
    if (dir) { setSettings((prev) => ({ ...prev, stableOsuDir: dir })); setMessage(`已选择 osu!stable 目录：${dir}`); }
  }

  async function scanSongs() {
    runBusy("正在扫描 Stable 曲库...", async () => {
      const result = await api.scanSongs(settings.songsDir);
      setLocalBeatmapsets(result.localBeatmapsets || {});
      setSettings((prev) => normalizeSettings({ ...prev, localSource: "stable" }));
      setLocalCount(result.count);
      setMessage(`Stable 扫描完成：此 Songs 文件夹识别到 ${result.count} 个 beatmapset。`);
    });
  }

  async function scanLazer() {
    runBusy("正在扫描 lazer 曲库...", async () => {
      const result = await api.scanLazer(settings.lazerDir);
      setLocalBeatmapsets(result.localBeatmapsets || {});
      setSettings((prev) => normalizeSettings({ ...prev, localSource: "lazer" }));
      setLocalCount(result.count);
      setMessage(`lazer 扫描完成：此 lazer 文件夹识别到 ${result.count} 个 beatmapset。`);
    });
  }

  async function refreshLocalLibrary() {
    if (settings.localSource === "lazer") {
      if (!settings.lazerDir) {
        setMessage("请先在设置页选择 osu! lazer 目录。");
        return;
      }
      await scanLazer();
      return;
    }
    if (!settings.songsDir) {
      setMessage("请先在设置页选择 Songs (Stable) 目录。");
      return;
    }
    await scanSongs();
  }

  async function scanCollections() {
    runBusy("正在扫描 stable 收藏夹...", async () => {
      const result = await api.scanStableCollections(settings.stableOsuDir);
      setStableCollections(result);
      setMessage(`收藏夹扫描完成：${result.length} 个收藏夹。`);
      const selectedName = settings.collectionName || result[0]?.name || "";
      if (!settings.collectionName && result[0]) updateSetting("collectionName", result[0].name);
      const selectedCollection = result.find((collection) => collection.name === selectedName) || result[0];
      if (selectedCollection) {
        setPlaylistSource("collection");
        setPlaylistMeta(null);
        setItems(selectedCollection.items || []);
        setSelectedIds(new Set((selectedCollection.items || []).map((item) => item.id)));
      }
    });
  }

  function openExportPlaylistDialog() {
    const selectedBeatmapIds = selectedPlaylistItems.flatMap((item) => item.collectionBeatmapIds?.length ? item.collectionBeatmapIds : item.beatmapIds);
    if (!selectedBeatmapIds.length) {
      setMessage("è¯·åå¨å³ä¾§æ­ååééå¾éè¦å¯¼åºçæ²ç®ã");
      return;
    }
    setPlaylistExportContext({ mode: "collection", sourceCollection: settings.collectionName || "æ¶èå¤¹" });
    setPlaylistExportDraft({ title: settings.collectionName || "Seekman Playlist", author: "", description: "" });
    setExportPlaylistOpen(true);
  }

  function openSearchExportDialog() {
    if (!selectedItems.length) {
      setMessage("è¯·åå¨åéåè¡¨éå¾éè¦å¯¼åºçæ²ç®ã");
      return;
    }
    setPlaylistExportContext({ mode: "search", sourceCollection: searchExportInfo.sourceCollection });
    setPlaylistExportDraft({ title: searchExportInfo.title, author: "", description: "" });
    setExportPlaylistOpen(true);
  }

  async function exportCollection() {
    runBusy("æ­£å¨å¯¼åºæ­å...", async () => {
      let path = "";
      if (playlistExportContext.mode === "collection") {
        const selectedBeatmapIds = selectedPlaylistItems.flatMap((item) => item.collectionBeatmapIds?.length ? item.collectionBeatmapIds : item.beatmapIds);
        if (!selectedBeatmapIds.length) {
          setMessage("è¯·åå¨å³ä¾§æ­ååééå¾éè¦å¯¼åºçæ²ç®ã");
          return;
        }
        path = await api.exportCollectionPlaylist(settings.stableOsuDir, settings.collectionName, selectedBeatmapIds, playlistExportDraft.title, playlistExportDraft.author, playlistExportDraft.description);
      } else {
        if (!selectedItems.length) {
          setMessage("è¯·åå¨åéåè¡¨éå¾éè¦å¯¼åºçæ²ç®ã");
          return;
        }
        path = await api.exportBeatmapsetPlaylist(selectedItems, playlistExportContext.sourceCollection, playlistExportDraft.title, playlistExportDraft.author, playlistExportDraft.description);
      }
      setExportPlaylistOpen(false);
      setMessage(path ? `æ­åå·²å¯¼åºï¼${path}` : "æ²¡æå¯¼åºæä»¶ã");
    });
  }

  async function importPlaylist() {
    runBusy("正在导入 Seekman 歌单...", async () => {
      const importedPlaylist = await api.importSeekmanPlaylist();
      const importedItems = importedPlaylist.items || [];
      const result = importedItems;
      if (!importedItems.length) {
        setMessage("没有导入曲目。");
        return;
      }
      setPlaylistMeta({ exportedAt: importedPlaylist.exportedAt || "", sourceCollection: importedPlaylist.sourceCollection || "", title: importedPlaylist.title || "", author: importedPlaylist.author || "", description: importedPlaylist.description || "" });
      setPlaylistSource("import");
      if (settings.collectionAutoAdd && settings.stableOsuDir && settings.collectionName) {
        const applied = await api.applyLocalPlaylistItemsToCollection(settings.stableOsuDir, settings.collectionName, importedItems);
        if (applied.missingItems.length) {
          setPendingPlaylistImport(applied);
          setItems(applied.missingItems);
          setSelectedIds(new Set(applied.missingItems.map((item) => item.id)));
          setMessage(`已把本地已有的 ${applied.appliedBeatmapsetCount} 个 beatmapset 写入收藏夹，还缺 ${applied.missingItems.length} 个 beatmapset。`);
          return;
        }
        setItems([]);
        setSelectedIds(new Set());
        setMessage(`已把本地已有的 ${applied.appliedBeatmapsetCount} 个 beatmapset 写入收藏夹，没有缺失项。`);
        return;
      }
      setItems(importedItems);
      setSelectedIds(new Set(importedItems.filter((item) => !item.existsLocal).map((item) => item.id)));
      setMessage(`歌单已导入：${result.length} 个 beatmapset，${result.filter((item) => item.existsLocal).length} 个已在本地。`);
    });
  }

  async function search() {
    runBusy("正在构建下图列表...", async () => {
      await saveSettings();
      const result = await api.searchBeatmapsets(filters);
      setPlaylistSource("search");
      setPlaylistMeta(null);
      setSearchExportInfo({ title: "æç´¢", sourceCollection: "æç´¢" });
      setItems(result);
      setSelectedIds(new Set(result.filter((item) => !item.existsLocal).map((item) => item.id)));
      setMessage(`列表构建完成：${result.length} 个结果，${result.filter((item) => item.existsLocal).length} 个已在本地。`);
    });
  }

  async function searchAlpha() {
    runBusy("正在获取 AlphaOsu! PP 推荐...", async () => {
      await saveSettings();
      const result = await api.searchAlphaRecommendations(alpha);
      setPlaylistSource("search");
      setPlaylistMeta(null);
      setSearchExportInfo({ title: "æ¨èppå¾", sourceCollection: "æ¨èppå¾" });
      setItems(result);
      setSelectedIds(new Set(result.filter((item) => !item.existsLocal).map((item) => item.id)));
      setMessage(`AlphaOsu! 推荐已载入：${result.length} 个结果，${result.filter((item) => item.existsLocal).length} 个已在本地。`);
    });
  }

  async function searchBestScores() {
    runBusy("正在获取玩家 BP...", async () => {
      await saveSettings();
      const result = await api.searchUserBestScores(best);
      setPlaylistSource("search");
      setPlaylistMeta(null);
      const bestName = best.username.trim() || "ç©å®¶";
      setSearchExportInfo({ title: `${bestName}çBP`, sourceCollection: `${bestName}çBP` });
      const modeChanged = lastBestMode !== null && lastBestMode !== best.mode;
      const baseItems = modeChanged ? [] : items;
      const baseSelectedIds = modeChanged ? new Set<number>() : selectedIds;
      const previousIds = new Set(baseItems.map((item) => item.id));
      const merged = new Map(baseItems.map((item) => [item.id, item]));
      result.forEach((item) => merged.set(item.id, item));
      setItems([...merged.values()]);
      setSelectedIds(new Set([...baseSelectedIds, ...result.filter((item) => !item.existsLocal).map((item) => item.id)]));
      setLastBestMode(best.mode);
      setMessage(`玩家 BP 已加入候选：${result.length} 个 beatmapset，新增 ${result.filter((item) => !previousIds.has(item.id)).length} 个，${result.filter((item) => item.existsLocal).length} 个已在本地。`);
    });
  }

  async function enqueue() {
    runBusy("正在加入下载队列...", async () => {
      await saveSettings();
      const nextTasks = await api.enqueueDownloads(selectedItems);
      setTasks(nextTasks);
      setActiveTab("downloads");
      setMessage(`已添加 1 个任务，包含 ${selectedItems.length} 首歌，等待手动开始；下载选项：${downloadModeLabel(settings.downloadMode)}。`);
    });
  }

  async function enqueueMissingPlaylistItems() {
    const pending = pendingPlaylistImport;
    if (!pending) return;
    setPendingPlaylistImport(null);
    runBusy("正在把缺失曲目加入下载任务...", async () => {
      await saveSettings();
      const nextTasks = await api.enqueueDownloads(pending.missingItems);
      setTasks(nextTasks);
      setActiveTab("downloads");
      setMessage(`已添加 1 个歌单补全任务，包含 ${pending.missingItems.length} 个缺失 beatmapset。下载完成后会统一写入目标收藏夹。`);
    });
  }

  async function processPlaylistSelection() {
    if (!selectedPlaylistItems.length) {
      setMessage("请先在歌单候选中选择要处理的曲目。");
      return;
    }
    if (!settings.collectionAutoAdd) {
      runBusy("正在把歌单缺失项加入下载任务...", async () => {
        await saveSettings();
        const downloadable = selectedPlaylistItems.filter((item) => !item.existsLocal);
        if (!downloadable.length) {
          setMessage("选中的曲目都已在本地。若要把它们写入另一个收藏夹，请先启用“下载完成后写入目标收藏夹”。");
          return;
        }
        const nextTasks = await api.enqueueDownloads(downloadable);
        setTasks(nextTasks);
        setActiveTab("downloads");
        setMessage(`已添加 1 个歌单下载任务，包含 ${downloadable.length} 个缺失 beatmapset。`);
      });
      return;
    }
    if (!settings.stableOsuDir || !settings.collectionName) {
      setMessage("请先选择 osu!stable 根目录和目标收藏夹。");
      return;
    }
    runBusy("正在检查歌单本地缺失...", async () => {
      await saveSettings();
      const preview = await api.applyLocalPlaylistItemsToCollection(settings.stableOsuDir, settings.collectionName, selectedPlaylistItems, false);
      setPendingPlaylistAction({ preview, items: selectedPlaylistItems });
      setMessage(`即将迁移 ${preview.appliedBeatmapsetCount} 个 beatmapset，缺失 ${preview.missingItems.length} 个 beatmapset。`);
    });
  }

  async function confirmPlaylistAction() {
    const pending = pendingPlaylistAction;
    if (!pending) return;
    setPendingPlaylistAction(null);
    runBusy("正在执行歌单迁移...", async () => {
      await saveSettings();
      const applied = await api.applyLocalPlaylistItemsToCollection(settings.stableOsuDir, settings.collectionName, pending.items, true);
      if (applied.missingItems.length) {
        const nextTasks = await api.enqueueDownloads(applied.missingItems);
        setTasks(nextTasks);
        setActiveTab("downloads");
        setItems(applied.missingItems);
        setSelectedIds(new Set(applied.missingItems.map((item) => item.id)));
        setMessage(`已迁移 ${applied.appliedBeatmapsetCount} 个 beatmapset，缺失的 ${applied.missingItems.length} 个 beatmapset 已加入下载任务。`);
        return;
      }
      setMessage(`已完成本地收藏夹迁移：${applied.appliedBeatmapsetCount} 个 beatmapset，共 ${applied.appliedCount} 个子难度已写入目标收藏夹。`);
    });
  }

  async function runBusy(label: string, fn: () => Promise<void>) {
    try { setBusy(label); setMessage(""); await fn(); }
    catch (error) { setMessage(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(""); }
  }

  function updateSetting(key: string, value: unknown) { setSettings((prev) => ({ ...prev, [key]: value })); }
  async function updateLocalSource(localSource: LocalSource) {
    const next = normalizeSettings({ ...settings, localSource });
    setSettings(next);
    setLocalCount(countLocalBySource(localBeatmapsets, localSource));
    const saved = await api.saveSettings(next);
    setSettings((prev) => normalizeSettings({ ...prev, ...saved }));
  }
  async function updateTheme(theme: string) {
    const next = normalizeSettings({ ...settings, theme });
    setSettings(next);
    const saved = await api.saveSettings(next);
    setSettings((prev) => normalizeSettings({ ...prev, ...saved }));
  }
  function updateDownloadMode(value: string) {
    setSettings((prev) => ({ ...prev, downloadMode: value, includeVideo: value === "video" }));
  }
  function updateFilter(key: string, value: string) { setFilters((prev) => ({ ...prev, [key]: value })); }
  function updateAlpha(key: string, value: string) { setAlpha((prev) => ({ ...prev, [key]: value })); }
  function updateBest(key: string, value: string) { setBest((prev) => ({ ...prev, [key]: value })); }
  function updateRange(minKey: keyof typeof defaultFilters, maxKey: keyof typeof defaultFilters, min: number, max: number) {
    setFilters((prev) => ({ ...prev, [minKey]: String(min), [maxKey]: String(max) }));
  }
  function moveMirror(index: number, direction: -1 | 1) {
    setSettings((prev) => {
      const priority = normalizeMirrorPriority(prev.mirrorPriority);
      const nextIndex = index + direction;
      if (nextIndex < 0 || nextIndex >= priority.length) return prev;
      [priority[index], priority[nextIndex]] = [priority[nextIndex], priority[index]];
      return { ...prev, mirrorPriority: priority };
    });
  }
  async function retryFailedDownloads() {
    await saveSettings();
    const nextTasks = await api.retryFailedDownloads();
    setTasks(nextTasks);
    if (nextTasks.length) await api.startDownloads();
    setMessage("已丢弃旧断点，并按当前镜像策略重新开始。");
  }
  async function startQueue() { await saveSettings(); await api.startDownloads(); setMessage("下载队列已开始。"); }
  async function pauseQueue() { await api.pauseDownloads(); const state = await api.getState(); setTasks(state.tasks || []); setMessage("下载队列已暂停。"); }
  async function clearAllDownloads() {
    setConfirmClearOpen(false);
    const nextTasks = await api.clearAllDownloads();
    setTasks(nextTasks);
    setMessage("下载队列已清空。");
  }
  async function deleteGroup(groupId: string) {
    setConfirmDeleteGroup(null);
    const nextTasks = await api.deleteDownloadGroup(groupId);
    setTasks(nextTasks);
    setMessage("任务已删除。");
  }
  async function forceFinishGroup(groupId: string) {
    setConfirmForceGroup(null);
    try {
      const nextTasks = await api.forceFinishDownloadGroup(groupId);
      setTasks(nextTasks);
      setMessage("任务已强制结束：已缓存完成的歌曲已提交，未完成项目已从任务中移除。");
    } catch (error) {
      setMessage(`强制结束失败：${String(error)}`);
    }
  }
  async function confirmEnableCollection() {
    const next = normalizeSettings({ ...settings, collectionAutoAdd: true });
    setCollectionRiskOpen(false);
    setSettings(next);
    const saved = await api.saveSettings(next);
    setSettings((prev) => normalizeSettings({ ...prev, ...saved }));
    setMessage("实验性收藏夹写入已启用。下载完成后会自动备份并写入 collection.db。");
  }
  async function installUpdate() {
    if (!updateInfo) return;
    setUpdating(true);
    try {
      await api.installUpdateNow();
      setMessage("安装包已下载并启动。请按安装器提示完成更新，用户设置会保留。");
      setUpdateInfo(null);
    } catch (error) {
      setMessage(`自动更新失败：${String(error)}。请打开发布页手动下载。`);
    } finally {
      setUpdating(false);
    }
  }
  async function dismissUpdate() {
    if (!updateInfo) return;
    await api.dismissUpdateVersion(updateInfo.version).catch(() => undefined);
    setUpdateInfo(null);
  }
  async function manualCheckUpdates() {
    runBusy("正在检查 GitHub 最新版本...", async () => {
      const info = await api.checkForUpdates();
      if (info) {
        setUpdateInfo(info);
        setMessage(`发现新版本 ${info.version}。`);
      } else {
        setMessage("当前已经是最新版本，或暂时无法连接 GitHub。");
      }
    });
  }
function toggleItem(id: number) { setSelectedIds((current) => { const next = new Set(current); next.has(id) ? next.delete(id) : next.add(id); return next; }); }
  function invertAvailableSelection() { setSelectedIds((current) => { const next = new Set(current); availableItems.forEach((item) => { next.has(item.id) ? next.delete(item.id) : next.add(item.id); }); return next; }); }
  function invertPlaylistSelection() { setSelectedIds((current) => { const next = new Set(current); playlistSelectableItems.forEach((item) => { next.has(item.id) ? next.delete(item.id) : next.add(item.id); }); return next; }); }
  function toggleGroup(groupId: string) { setExpandedGroups((current) => { const next = new Set(current); next.has(groupId) ? next.delete(groupId) : next.add(groupId); return next; }); }
  function selectExistingCollection(name: string) {
    setCollectionTargetMode("existing");
    updateSetting("collectionName", name);
    const collection = stableCollections.find((item) => item.name === name);
    if (collection) {
      setPlaylistSource("collection");
      setPlaylistMeta(null);
      setItems(collection.items || []);
      setSelectedIds(new Set((collection.items || []).map((item) => item.id)));
      setMessage(`已载入收藏夹：${name}，共 ${collection.beatmapCount} 个子难度。`);
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar app-nav">
        <div className="brand"><div className="brand-mark">o!</div><div><h1>Osu! Beatmap Seekman</h1></div></div>
        <nav className="nav-tabs" aria-label="主功能">
          <button className={activeTab === "settings" ? "active" : ""} onClick={() => setActiveTab("settings")}><Settings size={17} /> 设置</button>
          <button className={activeTab === "search" ? "active" : ""} onClick={() => setActiveTab("search")}><Search size={17} /> 搜图</button>
          <button className={activeTab === "downloads" ? "active" : ""} onClick={() => setActiveTab("downloads")}><Download size={17} /> 下载</button>
          <button className={activeTab === "playlists" ? "active" : ""} onClick={() => setActiveTab("playlists")}><FolderOpen size={17} /> 歌单</button>
        </nav>
        <div className="nav-summary">
          <div className="nav-summary-head"><span>本地识别</span><button type="button" onClick={refreshLocalLibrary} disabled={Boolean(busy)} aria-label="刷新本地曲库"><RotateCcw size={14} /></button></div>
          <strong>{localCount}</strong>
          <small>{settings.localSource === "lazer" ? "lazer 去重" : "Stable 去重"}</small>
        </div>
      </aside>
      <section className="workspace">
        <header className="toolbar"><div><h2>{tabTitle(activeTab)}</h2><p>{busy || message || "写好搜索条件就可以构建列表。"}</p></div></header>

        {activeTab === "settings" && <section className="page-grid settings-grid">
          <section className="panel">
            <div className="panel-heading"><h2><FolderOpen size={17} /> 目录设置</h2><p>选择曲库位置，并决定本地去重使用 Stable 还是 lazer。</p></div>
            <button className="primary" onClick={selectSongsDir}><FolderOpen size={16} /> 选择 Songs (Stable)</button>
            <div className="path-box">{settings.songsDir || "尚未选择"}</div>
            <button className="ghost" onClick={scanSongs} disabled={!settings.songsDir || Boolean(busy)}><RotateCcw size={16} /> 扫描 Stable 曲库</button>
            <button className="primary" onClick={selectLazerDir}><FolderOpen size={16} /> 选择 osu! lazer</button>
            <div className="path-box">{settings.lazerDir || "尚未选择"}</div>
            <button className="ghost" onClick={scanLazer} disabled={!settings.lazerDir || Boolean(busy)}><RotateCcw size={16} /> 扫描 lazer 曲库</button>
            <p className="hint">扫描 lazer 曲库需要等待大约一分钟；lazer 图需要在 osu!lazer 中手动导入。</p>
            <div className="local-source-toggle" role="group" aria-label="本地去重来源">
              <button type="button" className={settings.localSource === "stable" ? "active" : ""} onClick={() => updateLocalSource("stable")}>Stable</button>
              <button type="button" className={settings.localSource === "lazer" ? "active" : ""} onClick={() => updateLocalSource("lazer")}>lazer</button>
            </div>
            <label className="check-row"><input type="checkbox" checked={settings.hideExisting} onChange={(e) => updateSetting("hideExisting", e.target.checked)} /><span>隐藏已有图</span></label>
          </section>
          <section className="panel">
            <div className="panel-heading"><h2><Settings size={17} /> osu! API <button className="icon-help" type="button" onClick={() => setApiHelpOpen(true)} aria-label="osu! API 导入教程"><HelpCircle size={14} /></button></h2><p>填写官方 API 信息，用于搜索和筛选 beatmapset。</p></div>
            <button className="ghost" type="button" onClick={() => api.openApiPage()}>获取 API</button>
            <label>Client ID<input value={settings.osuClientId} onChange={(e) => updateSetting("osuClientId", e.target.value)} /></label>
            <label>Client Secret<input type="password" value={settings.osuClientSecret} onChange={(e) => updateSetting("osuClientSecret", e.target.value)} /></label>
            <label>Bearer Token<input type="password" value={settings.bearerToken} onChange={(e) => updateSetting("bearerToken", e.target.value)} /></label>
            <label>并发下载<input type="number" min={1} max={64} value={settings.concurrentDownloads} onChange={(e) => updateSetting("concurrentDownloads", Number(e.target.value))} /></label>
            <button className="ghost" onClick={() => saveSettings().then(() => setMessage("设置已保存。"))}>保存设置</button>
          </section>
          <section className="panel">
            <div className="panel-heading"><h2><Download size={17} /> 镜像源设置</h2><p>调整镜像优先级；任务重试时会按当前优先级重新选择源。</p></div>
            <label className="check-row"><input type="checkbox" checked={settings.mixedMode} onChange={(e) => updateSetting("mixedMode", e.target.checked)} /><span>混杂模式</span></label>
            <div className="mirror-list">{normalizeMirrorPriority(settings.mirrorPriority).map((mirror, index) => (
              <div className="mirror-row" key={mirror}><span>{index + 1}. {mirrorLabels[mirror]}</span><div><button type="button" onClick={() => moveMirror(index, -1)} disabled={index === 0}>↑</button><button type="button" onClick={() => moveMirror(index, 1)} disabled={index === defaultMirrorPriority.length - 1}>↓</button></div></div>
            ))}</div>
            <p className="hint">如果下载卡住，先把更流畅的镜像源移到最上方，再点击下载页里的“一键重试”。</p>
          </section>
          <section className="panel">
            <div className="panel-heading"><h2><Palette size={17} /> 主题设置</h2><p>选择界面配色，设置会自动记住。</p></div>
            <div className="theme-options">{themeOptions.map((theme) => (
              <button className={`theme-swatch ${normalizeTheme(settings.theme) === theme.id ? "active" : ""}`} type="button" key={theme.id} onClick={() => updateTheme(theme.id)}>
                <span className="theme-dots"><i style={{ background: theme.primary }} /><i style={{ background: theme.surface }} /></span><span>{theme.label}</span>
              </button>
            ))}</div>
            <button className="ghost" type="button" onClick={manualCheckUpdates} disabled={Boolean(busy)}><RotateCcw size={16} /> 检查 GitHub 更新</button>
            <p className="hint">启动时会自动检查最新 Release；没有更新或无法连接时不会弹窗。Windows 可从 GitHub 下载安装到最新版，用户设置会保留。</p>
          </section>
        </section>}


        {activeTab === "search" && <>
        <section className="filters">
          <div className="filter-row filter-row-primary">
            <label className="filter-query"><span className="filter-label-row"><Search size={15} /> 关键词<button className="icon-help" type="button" onClick={() => setSearchHelpOpen(true)} aria-label="搜索关键词说明">?</button></span><input value={filters.query} onChange={(e) => updateFilter("query", e.target.value)} /></label>
            <label>状态<select value={filters.status} onChange={(e) => updateFilter("status", e.target.value)}><option value="ranked">Ranked</option><option value="loved">Loved</option><option value="graveyard">Graveyard</option></select></label>
            <label>模式<select value={filters.mode} onChange={(e) => updateFilter("mode", e.target.value)}><option value="any">全部</option><option value="osu">osu</option><option value="taiko">taiko</option><option value="fruits">fruits</option><option value="mania">mania</option></select></label>
            <label>页数<input value={filters.maxPages} onChange={(e) => updateFilter("maxPages", e.target.value)} /></label>
            <label>排序<select value={filters.sortBy} onChange={(e) => updateFilter("sortBy", e.target.value)}><option value="time">时间</option><option value="stars">星数</option><option value="relevance">相关性</option><option value="length">时长</option><option value="bpm">BPM</option></select></label>
            <label>方向<select value={filters.sortDir} onChange={(e) => updateFilter("sortDir", e.target.value)}><option value="desc">倒序</option><option value="asc">正序</option></select></label>
          </div>
          <div className="filter-row filter-row-secondary">
            <label><CalendarDays size={15} /> 起始日期<input type="date" value={filters.dateFrom} onChange={(e) => updateFilter("dateFrom", e.target.value)} /></label>
            <label><CalendarDays size={15} /> 结束日期<input type="date" value={filters.dateTo} onChange={(e) => updateFilter("dateTo", e.target.value)} /></label>
            <label>最短秒数<input value={filters.minLength} onChange={(e) => updateFilter("minLength", e.target.value)} placeholder="可空" /></label>
            <label>最长秒数<input value={filters.maxLength} onChange={(e) => updateFilter("maxLength", e.target.value)} placeholder="可空" /></label>
          </div>
          <RangeSlider label="SR" min={0} max={15} step={0.1} valueMin={Number(filters.minStars)} valueMax={Number(filters.maxStars)} onChange={(min, max) => updateRange("minStars", "maxStars", min, max)} />
          <details className="advanced-filters"><summary><Gauge size={15} /> OD / HP / CS / AR / BPM / mania 键数</summary><div className="advanced-grid">
            <RangeSlider label="OD" min={0} max={10} step={0.1} valueMin={Number(filters.minOd)} valueMax={Number(filters.maxOd)} onChange={(min, max) => updateRange("minOd", "maxOd", min, max)} />
            <RangeSlider label="HP" min={0} max={10} step={0.1} valueMin={Number(filters.minHp)} valueMax={Number(filters.maxHp)} onChange={(min, max) => updateRange("minHp", "maxHp", min, max)} />
            <RangeSlider label="CS" min={0} max={10} step={0.1} valueMin={Number(filters.minCs)} valueMax={Number(filters.maxCs)} onChange={(min, max) => updateRange("minCs", "maxCs", min, max)} />
            <RangeSlider label="AR" min={0} max={10} step={0.1} valueMin={Number(filters.minAr)} valueMax={Number(filters.maxAr)} onChange={(min, max) => updateRange("minAr", "maxAr", min, max)} />
            <RangeSlider label="BPM" min={0} max={400} step={1} valueMin={Number(filters.minBpm)} valueMax={Number(filters.maxBpm)} onChange={(min, max) => updateRange("minBpm", "maxBpm", min, max)} />
            <label className="advanced-key-select">mania 键数<select value={filters.keyCount} onChange={(e) => updateFilter("keyCount", e.target.value)} disabled={filters.mode !== "mania"}><option value="any">全部</option><option value="4">4K</option><option value="7">7K</option></select></label>
          </div></details>
          <details className="alpha-panel">
            <summary>AlphaOsu! PP 推荐</summary>
            <div className="filter-row alpha-row">
              <label>用户名 / ID<input value={alpha.username} onChange={(e) => updateAlpha("username", e.target.value)} placeholder={featuredPlayerPlaceholder(alpha.mode)} /></label>
              <label>数量<input value={alpha.limit} onChange={(e) => updateAlpha("limit", e.target.value)} /></label>
              <label>模式<select value={alpha.mode} onChange={(e) => updateAlpha("mode", e.target.value)}><option value="mania">mania</option><option value="osu">osu</option></select></label>
              <label>mania 键数<select value={alpha.keyCount} onChange={(e) => updateAlpha("keyCount", e.target.value)} disabled={alpha.mode !== "mania"}><option value="4">4K</option><option value="7">7K</option></select></label>
              <button className="primary" onClick={searchAlpha} disabled={!alpha.username.trim() || Boolean(busy)}><Search size={16} /> 获取推荐</button>
            </div>
          </details>
          <details className="alpha-panel">
            <summary>抄玩家 BP</summary>
            <div className="filter-row alpha-row">
              <label>玩家 ID / 用户名<input value={best.username} onChange={(e) => updateBest("username", e.target.value)} placeholder={featuredPlayerPlaceholder(best.mode)} /></label>
              <label>前 N 首<input value={best.limit} onChange={(e) => updateBest("limit", e.target.value)} /></label>
              <label>模式<select value={best.mode} onChange={(e) => updateBest("mode", e.target.value)}><option value="osu">osu</option><option value="taiko">taiko</option><option value="fruits">fruits</option><option value="mania">mania</option></select></label>
              <button className="primary" onClick={searchBestScores} disabled={!best.username.trim() || Boolean(busy)}><Search size={16} /> 获取 BP</button>
            </div>
          </details>
        </section>
        <div className="actions"><button className="primary" onClick={search} disabled={Boolean(busy)}><Search size={16} /> 构建列表</button><label className="inline-select">下载版本<select value={settings.downloadMode} onChange={(e) => updateDownloadMode(e.target.value)}><option value="video">带视频 .osz</option><option value="noVideo">不带视频 .osz</option><option value="osu">仅 .osu 文件</option></select></label><button onClick={enqueue} disabled={!selectedItems.length || Boolean(busy)}><Download size={16} /> 添加任务</button><span>{selectedItems.length} 首待加入，当前任务已下载 {formatBytes(selectedDownloaded)}</span></div>
        <section className="content-grid single-column">
          <div className="table-panel"><div className="table-head"><strong>候选列表</strong><div className="table-head-actions"><button onClick={() => setSelectedIds(new Set(availableItems.map((item) => item.id)))}>全选可下载</button><button onClick={invertAvailableSelection}>全反选</button><button onClick={openSearchExportDialog} disabled={!selectedItems.length || Boolean(busy)}><FolderOpen size={16} /> 导出为歌单</button></div></div><div className="table">{visibleItems.map((item) => <label className={`row ${item.existsLocal ? "muted" : ""}`} key={item.id}><input type="checkbox" checked={selectedIds.has(item.id)} disabled={item.existsLocal} onChange={() => toggleItem(item.id)} /><div className="main-cell"><strong>{item.artist} - {item.title}</strong><span>#{item.id} ? {item.status} ? {renderCreator(item.creator)} ? {formatDate(item.rankedDate)} ? {item.modes.join(", ")}{item.keyCounts.length ? ` ? ${item.keyCounts.join("/")}K` : ""}</span></div><div>{formatStars(item)}</div><div>{formatOdHp(item)}</div><div>{formatCsArBpm(item)}</div><div>{formatLength(item)}</div><div>{item.existsLocal ? "已存在" : "可下载"}</div></label>)}{!visibleItems.length && <div className="empty">还没有列表。</div>}</div></div>
        </section>
        </>}

        {activeTab === "downloads" && <section className="queue-panel task-page"><div className="queue-summary"><div className="queue-summary-main"><strong>下载任务</strong><span>{overall.completed}/{overall.total} · 已下载 {formatBytes(overall.downloadedBytes)}</span></div><div className="creator-note"><span>软件作者：凛澪 · <button className="inline-link" type="button" onClick={() => api.openExternalUrl("https://osu.ppy.sh/users/12505146")}>我的 Osu 主页</button></span><span>广告位：来看一下我主办的全国高校 Osu!Mania 大赛 CUC 吧！</span><span><button className="inline-link" type="button" onClick={() => api.openExternalUrl("https://www.bilibili.com/video/BV133SDBQEdP/?spm_id_from=333.337.search-card.all.click")}>往届赛事录像</button> · 群号：1062134328，欢迎高校 4K 选手与主模式 / 7K Staff 加入</span></div></div><p className="hint">任务从前往后依次处理；点开任务可以查看里面的具体下载项目。</p><div className="queue-actions queue-actions-row"><button className="primary" onClick={startQueue} disabled={!tasks.length}><Play size={16} /> 开始</button><button onClick={pauseQueue} disabled={!tasks.some((task) => task.status === "downloading")}><Pause size={16} /> 暂停</button><button onClick={retryFailedDownloads} disabled={!tasks.length}>一键重试</button><button onClick={() => setConfirmClearOpen(true)} disabled={!tasks.length}>清除所有</button></div><div className={`overall-bar ${overall.isActiveUnknown ? "indeterminate" : ""}`}><div style={{ width: `${overall.percent}%` }} /></div><div className="queue-list group-list">{taskGroups.map((group) => <div className="task-group" key={group.id}><div className="task-group-head"><button className="task-group-toggle" type="button" onClick={() => toggleGroup(group.id)}><span><strong>{group.name}</strong><small>{group.source} · {group.destination}</small></span><span>{group.completed}/{group.total} · {formatBytes(group.downloadedBytes)}</span></button><div className="task-group-actions"><button className="subtle-danger" type="button" onClick={() => setConfirmForceGroup(group.id)}>强制结束</button><button className="danger subtle-danger" type="button" onClick={() => setConfirmDeleteGroup(group.id)}>删除</button></div></div><div className="task-bar"><div style={{ width: `${group.percent}%` }} /></div>{expandedGroups.has(group.id) && <div className="group-items">{group.tasks.map((task) => { const percent = task.totalBytes ? Math.floor((task.downloadedBytes / task.totalBytes) * 100) : 0; const isUnknownActive = !task.totalBytes && task.status === "downloading"; const visiblePercent = task.downloadedBytes > 0 && task.status === "downloading" ? Math.max(percent, 2) : percent; return <div className="task" key={task.id}><div className="task-title"><strong>{task.artist} - {task.title}</strong><span>{statusText(task.status)}</span></div><div className={`task-bar ${isUnknownActive ? "indeterminate" : ""}`}><div style={{ width: `${isUnknownActive ? 100 : Math.min(visiblePercent, 100)}%` }} /></div><div className="task-meta">已下载 {formatBytes(task.downloadedBytes)}{task.totalBytes ? ` / ${formatBytes(task.totalBytes)}` : ""} · 源 {mirrorNameFromUrl(task.url)} · {downloadModeLabel(task.downloadMode)}{task.collectionBeatmapIds?.length ? ` · 收藏夹子难度 ${task.collectionBeatmapIds.length}` : ""}{task.beatmapId ? ` · #${task.beatmapId}` : ""}{task.error && ` · ${task.error}`}</div></div>; })}</div>}</div>)}{!taskGroups.length && <div className="empty">还没有任务。</div>}</div></section>}

                {activeTab === "playlists" && <section className="page-grid playlist-grid">
          <section className="panel">
            <h2><FolderOpen size={17} /> 歌单</h2>
            <label>osu!stable 根目录<input value={settings.stableOsuDir} onChange={(e) => updateSetting("stableOsuDir", e.target.value)} placeholder="D:\\osu!std" /></label>
            <button className="ghost" type="button" onClick={selectStableOsuDir}><FolderOpen size={16} /> 选择 osu!stable</button>
            <button className="ghost" type="button" onClick={scanCollections} disabled={!settings.stableOsuDir || Boolean(busy)}><RotateCcw size={16} /> 扫描收藏夹</button>
            <div className="local-source-toggle" role="group" aria-label="收藏夹目标">
              <button type="button" className={collectionTargetMode === "existing" ? "active" : ""} onClick={() => setCollectionTargetMode("existing")}>已有收藏夹</button>
              <button type="button" className={collectionTargetMode === "new" ? "active" : ""} onClick={() => setCollectionTargetMode("new")}>新建收藏夹</button>
            </div>
            {collectionTargetMode === "existing" && stableCollections.length > 0 && <label>选择已有收藏夹<select value={settings.collectionName} onChange={(e) => selectExistingCollection(e.target.value)}>{stableCollections.map((collection) => <option value={collection.name} key={collection.name}>{collection.name} ({collection.beatmapCount})</option>)}</select></label>}
            {collectionTargetMode === "existing" && !stableCollections.length && <p className="hint">先扫描收藏夹后可以选择已有收藏夹。</p>}
            {collectionTargetMode === "new" && <label>新收藏夹名称<input value={settings.collectionName} onChange={(e) => updateSetting("collectionName", e.target.value)} placeholder="Seekman Downloads" /></label>}
            <label className="check-row"><input type="checkbox" checked={settings.collectionAutoAdd} onChange={(e) => e.target.checked ? setCollectionRiskOpen(true) : updateSetting("collectionAutoAdd", false)} /><span>下载完成后写入目标收藏夹</span></label>
            <div className="playlist-action-row">
              <button className="ghost" type="button" onClick={openExportPlaylistDialog} disabled={!settings.stableOsuDir || !settings.collectionName || Boolean(busy)}>导出歌单</button>
              <button className="icon-help" type="button" onClick={() => setPlaylistPathsHelpOpen(true)} aria-label="歌单导出位置说明"><HelpCircle size={14} /></button>
            </div>
            <button className="primary" type="button" onClick={importPlaylist} disabled={Boolean(busy)}>导入歌单</button>
            <p className="hint">从歌单添加任务时，会保留源收藏夹中的具体子难度；写入新收藏夹时不会把整张图所有难度都加入。</p>
          </section>
          <section className="table-panel">
            <div className="table-head">
              <div className="playlist-head-main">
                <strong>歌单候选</strong>
                {playlistMeta && (
                  <div className="playlist-created">
                    {playlistMeta.title && (
                      <div className="playlist-meta-title">{playlistMeta.title}</div>
                    )}
                    <div className="playlist-meta-grid">
                      {playlistMeta.exportedAt && <span><strong>创建时间</strong>{formatPlaylistCreatedAt(playlistMeta.exportedAt)}</span>}
                      {playlistMeta.author && <span><strong>作者</strong>{playlistMeta.author}</span>}
                      {playlistMeta.sourceCollection && <span><strong>来源收藏夹</strong>{playlistMeta.sourceCollection}</span>}
                    </div>
                    {playlistMeta.description && <p>{playlistMeta.description}</p>}
                  </div>
                )}
              </div>
              <div className="table-head-actions">
                <button onClick={() => setSelectedIds(new Set(playlistSelectableItems.map((item) => item.id)))}>全选可下载</button>
                <button onClick={invertPlaylistSelection}>全反选</button>
                <button onClick={processPlaylistSelection} disabled={!selectedPlaylistItems.length || Boolean(busy)}><Download size={16} /> 添加任务</button>
              </div>
            </div>
            <div className="table">
              {playlistVisibleItems.map((item) => <label className={`row ${item.existsLocal ? "muted" : ""}`} key={item.id}><input type="checkbox" checked={selectedIds.has(item.id)} disabled={item.existsLocal && playlistSource === "search"} onChange={() => toggleItem(item.id)} /><div className="main-cell"><strong>{item.artist} - {item.title}</strong><span>#{item.id} · {item.sourceCollection ? `来自 ${item.sourceCollection}` : item.status} · {item.collectionBeatmapIds?.length ? `收藏夹子难度 ${item.collectionBeatmapIds.length}` : item.modes.join(", ")}</span></div><div>{formatStars(item)}</div><div>{formatOdHp(item)}</div><div>{formatCsArBpm(item)}</div><div>{formatLength(item)}</div><div>{item.existsLocal ? "已存在" : "可下载"}</div></label>)}
              {!playlistVisibleItems.length && <div className="empty">导入歌单后会显示在这里。</div>}
            </div>
          </section>
        </section>}
      </section>
      {confirmClearOpen && <div className="modal-backdrop" role="presentation" onClick={() => setConfirmClearOpen(false)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="clear-queue-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="clear-queue-title">清除下载队列？</h2>
          <p>这会移除当前下载队列中的所有任务，正在下载的任务也会被取消。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setConfirmClearOpen(false)}>取消</button>
            <button className="primary danger" type="button" onClick={clearAllDownloads}>确认清除</button>
          </div>
        </div>
      </div>}
      {confirmDeleteGroup && <div className="modal-backdrop" role="presentation" onClick={() => setConfirmDeleteGroup(null)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="delete-task-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="delete-task-title">删除这个下载任务？</h2>
          <p>这会移除此任务里的所有下载项目，正在下载或已缓存但尚未提交的项目也会被取消。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setConfirmDeleteGroup(null)}>取消</button>
            <button className="primary danger" type="button" onClick={() => deleteGroup(confirmDeleteGroup)}>确认删除</button>
          </div>
        </div>
      </div>}
      {confirmForceGroup && <div className="modal-backdrop" role="presentation" onClick={() => setConfirmForceGroup(null)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="force-task-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="force-task-title">强制结束这个任务？</h2>
          <p>这会把此任务中已经下载并缓存完成的歌曲立即转移到 Songs，并写入目标收藏夹；失败、卡住、排队中或尚未完成的项目会从任务中移除。</p>
          <p>适合歌单任务只剩少数歌曲一直失败时使用。建议先确认 osu!stable 没有运行。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setConfirmForceGroup(null)}>取消</button>
            <button className="primary danger" type="button" onClick={() => forceFinishGroup(confirmForceGroup)}>确认强制结束</button>
          </div>
        </div>
      </div>}
      {pendingPlaylistAction && <div className="modal-backdrop" role="presentation" onClick={() => setPendingPlaylistAction(null)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="playlist-action-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="playlist-action-title">确认处理歌单？</h2>
          <p>即将把本地已有的 {pendingPlaylistAction.preview.appliedBeatmapsetCount} 个 beatmapset 写入目标收藏夹，共 {pendingPlaylistAction.preview.appliedCount} 个子难度。</p>
          <p>还缺失 {pendingPlaylistAction.preview.missingItems.length} 个 beatmapset。确认后，已有曲目会立即迁移；缺失曲目会加入下载任务，下载完成后再写入收藏夹。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setPendingPlaylistAction(null)}>取消</button>
            <button className="primary" type="button" onClick={confirmPlaylistAction}>确认处理</button>
          </div>
        </div>
      </div>}
      {pendingPlaylistImport && <div className="modal-backdrop" role="presentation" onClick={() => setPendingPlaylistImport(null)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="playlist-import-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="playlist-import-title">歌单需要补全下载</h2>
          <p>已经在本地找到并写入 {pendingPlaylistImport.appliedBeatmapsetCount} 个 beatmapset，共 {pendingPlaylistImport.appliedCount} 个子难度。</p>
          <p>还有 {pendingPlaylistImport.missingItems.length} 个 beatmapset 缺失。是否把缺失部分加入下载任务？下载完成后会再统一迁移到目标收藏夹。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setPendingPlaylistImport(null)}>暂不下载</button>
            <button className="primary" type="button" onClick={enqueueMissingPlaylistItems}>加入下载任务</button>
          </div>
        </div>
      </div>}
      {exportPlaylistOpen && <div className="modal-backdrop" role="presentation" onClick={() => setExportPlaylistOpen(false)}>
        <div className="confirm-dialog playlist-export-dialog" role="dialog" aria-modal="true" aria-labelledby="playlist-export-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="playlist-export-title">导出歌单</h2>
          <p>标题会预填为当前收藏夹名称；作者和简介可选，都会一并写入歌单 metadata，方便别人导入时查看来源。</p>
          <label>
            标题
            <input value={playlistExportDraft.title} onChange={(event) => setPlaylistExportDraft((current) => ({ ...current, title: event.target.value }))} placeholder={settings.collectionName || "Seekman Playlist"} />
          </label>
          <label>
            作者
            <input value={playlistExportDraft.author} onChange={(event) => setPlaylistExportDraft((current) => ({ ...current, author: event.target.value }))} placeholder="可选" />
          </label>
          <label>
            简介
            <textarea value={playlistExportDraft.description} onChange={(event) => setPlaylistExportDraft((current) => ({ ...current, description: event.target.value }))} rows={4} placeholder="可选" />
          </label>
          <div className="confirm-actions">
            <button type="button" onClick={() => setExportPlaylistOpen(false)}>取消</button>
            <button className="primary" type="button" onClick={exportCollection} disabled={!playlistExportDraft.title.trim()}>确认导出</button>
          </div>
        </div>
      </div>}
      {playlistPathsHelpOpen && <div className="modal-backdrop" role="presentation" onClick={() => setPlaylistPathsHelpOpen(false)}>
        <div className="confirm-dialog playlist-paths-dialog" role="dialog" aria-modal="true" aria-labelledby="playlist-paths-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="playlist-paths-title">歌单与备份保存位置</h2>
          <p>导出的歌单 CSV 会放到软件根目录下的 <strong>seekman-playlists</strong> 文件夹。</p>
          <p>如果启用了收藏夹写入，程序会在你选择的 osu!stable 根目录里备份 <strong>collection.db</strong>，文件名格式为 <strong>collection.db.seekman-backup-时间戳</strong>。</p>
          <p>这样就算后面要回滚收藏夹，或者想把歌单文件发给别人，也都比较好找。</p>
          <div className="confirm-actions">
            <button className="primary" type="button" onClick={() => setPlaylistPathsHelpOpen(false)}>知道了</button>
          </div>
        </div>
      </div>}
      {searchHelpOpen && <div className="modal-backdrop" role="presentation" onClick={() => setSearchHelpOpen(false)}>
        <div className="confirm-dialog search-help-dialog" role="dialog" aria-modal="true" aria-labelledby="search-help-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="search-help-title">搜索关键词</h2>
          <p>示例：<strong>creator=Linn0</strong> 会搜索所有 Linn0 写的图。</p>
          <div className="keyword-help-list">
            <div><strong>artist</strong><span>作曲家的名字</span></div>
            <div><strong>creator</strong><span>谱面难度的作者</span></div>
            <div><strong>title</strong><span>歌曲名</span></div>
            <div><strong>source</strong><span>歌曲的媒体，比如电子游戏、电影、系列、活动，也就是歌曲的来源或最相关的东西</span></div>
            <div><strong>tag</strong><span>特定的玩家标签</span></div>
          </div>
          <div className="confirm-actions">
            <button className="primary" type="button" onClick={() => setSearchHelpOpen(false)}>知道了</button>
          </div>
        </div>
      </div>}
      {apiHelpOpen && <div className="modal-backdrop" role="presentation" onClick={() => setApiHelpOpen(false)}>
        <div className="confirm-dialog api-help-dialog" role="dialog" aria-modal="true" aria-labelledby="api-help-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="api-help-title">osu! API 导入教程</h2>
          <ol className="api-help-steps">
            <li>点击设置页里的“获取 API”，打开 osu! 账号设置页面。</li>
            <li>在“拥有的客户端”区域点击“新的 OAuth 应用”。</li>
            <li>应用名称可以填写 <strong>seekman</strong>，回调链接留空，然后点击“注册应用程序”。</li>
            <li>注册完成后，把页面显示的 <strong>Client ID</strong> 和 <strong>Client Secret</strong> 分别填回软件并保存设置。</li>
            <li>Bearer Token 可以不填，软件会根据 Client ID / Secret 自动获取。</li>
          </ol>
          <div className="api-guide-images">
            <img src="/api-guide/oauth-list.png" alt="在 osu! 账号设置中点击新的 OAuth 应用" />
            <img src="/api-guide/oauth-register.png" alt="注册新的 OAuth 应用程序" />
          </div>
          <div className="confirm-actions">
            <button type="button" onClick={() => api.openApiPage()}>打开 API 页面</button>
            <button className="primary" type="button" onClick={() => setApiHelpOpen(false)}>知道了</button>
          </div>
        </div>
      </div>}
      {updateInfo && <div className="modal-backdrop" role="presentation" onClick={() => setUpdateInfo(null)}>
        <div className="confirm-dialog update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="update-title">发现新版本 {updateInfo.version}</h2>
          <p><strong>{updateInfo.name}</strong></p>
          {updateInfo.publishedAt && <p>发布时间：{updateInfo.publishedAt.slice(0, 10)}</p>}
          <div className="update-notes">{updateInfo.body || "这个版本没有填写更新公告。"}</div>
          <div className="confirm-actions">
            {updateInfo.canInstallNow && <button className="primary" type="button" onClick={installUpdate} disabled={updating}>{updating ? "正在下载..." : "立即更新"}</button>}
            <button type="button" onClick={() => api.openExternalUrl(updateInfo.htmlUrl)}>查看发布页</button>
            <button type="button" onClick={dismissUpdate}>不再提示</button>
            <button type="button" onClick={() => setUpdateInfo(null)}>稍后</button>
          </div>
        </div>
      </div>}
      {collectionRiskOpen && <div className="modal-backdrop" role="presentation" onClick={() => setCollectionRiskOpen(false)}>
        <div className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="collection-risk-title" onClick={(event) => event.stopPropagation()}>
          <h2 id="collection-risk-title">启用实验性收藏夹写入？</h2>
          <p>这个功能会在 .osz 下载完成后修改 osu!stable 根目录下的 collection.db，把曲目写入当前目标收藏夹。程序会先自动备份，但如果游戏正在运行、路径选错，或 osu! 后续数据库格式变化，仍可能导致收藏夹异常。</p>
          <p>建议先关闭 osu!stable，并确认根目录是包含 collection.db 和 Songs 文件夹的 osu!stable 目录。</p>
          <div className="confirm-actions">
            <button type="button" onClick={() => setCollectionRiskOpen(false)}>取消</button>
            <button className="primary danger" type="button" onClick={confirmEnableCollection}>我理解风险，启用</button>
          </div>
        </div>
      </div>}
    </main>
  );
}

function RangeSlider({ label, min, max, step, valueMin, valueMax, onChange }: { label: string; min: number; max: number; step: number; valueMin: number; valueMax: number; onChange: (min: number, max: number) => void; }) {
  const low = clamp(valueMin, min, max); const high = clamp(valueMax, min, max); const left = ((low - min) / (max - min)) * 100; const right = 100 - ((high - min) / (max - min)) * 100;
  return <div className="range-card"><div className="range-head"><span>{label}</span><strong>{low.toFixed(1)} - {high.toFixed(1)}</strong></div><div className="dual-range" style={{ "--range-left": `${left}%`, "--range-right": `${right}%` } as React.CSSProperties}><input type="range" min={min} max={max} step={step} value={low} onChange={(e) => onChange(Math.min(Number(e.target.value), high), high)} /><input type="range" min={min} max={max} step={step} value={high} onChange={(e) => onChange(low, Math.max(Number(e.target.value), low))} /></div></div>;
}
function formatPlaylistCreatedAt(value: string) { if (!value) return "未知"; const date = new Date(value); return Number.isNaN(date.getTime()) ? value : date.toLocaleString(); }
function clamp(value: number, min: number, max: number) { return Math.min(Math.max(value, min), max); }
function featuredPlayerPlaceholder(mode: Mode) { if (mode === "osu") return "mrekk"; if (mode === "mania") return "saragi"; return "玩家 ID / 用户名"; }
function formatBytes(value: number | null | undefined) { if (!value) return "0 MB"; const mb = value / 1024 / 1024; return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`; }
function formatDate(value: string) { return value ? value.slice(0, 10) : "未知日期"; }
function formatStars(item: BeatmapsetItem) { return item.minStars === null || item.maxStars === null ? "未知星数" : `${item.minStars.toFixed(2)}-${item.maxStars.toFixed(2)}*`; }
function formatOdHp(item: BeatmapsetItem) { return item.minOd === null || item.maxOd === null || item.minHp === null || item.maxHp === null ? "OD/HP 未知" : `OD ${item.minOd.toFixed(1)}-${item.maxOd.toFixed(1)} · HP ${item.minHp.toFixed(1)}-${item.maxHp.toFixed(1)}`; }
function formatCsArBpm(item: BeatmapsetItem) { const cs = item.minCs !== null && item.maxCs !== null ? `CS ${item.minCs.toFixed(1)}-${item.maxCs.toFixed(1)}` : "CS ?"; const ar = item.minAr !== null && item.maxAr !== null ? `AR ${item.minAr.toFixed(1)}-${item.maxAr.toFixed(1)}` : "AR ?"; const bpm = item.minBpm !== null && item.maxBpm !== null ? `BPM ${Math.round(item.minBpm)}-${Math.round(item.maxBpm)}` : "BPM ?"; return `${cs} · ${ar} · ${bpm}`; }
function formatLength(item: BeatmapsetItem) { const seconds = item.maxLength || item.minLength; if (!seconds) return "未知时长"; return `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`; }
function renderCreator(value: string) {
  if (value.includes("BP #")) {
    return <span className="bp-meta">{value.split(" 路 ").map((part, index) => {
      const isPp = /^[\d.]+pp$/i.test(part);
      const isMod = index > 3 && /^[A-Z0-9+]+$/.test(part) && part !== "ACC";
      return <span className={isPp ? "bp-pp" : isMod ? "bp-mod" : ""} key={`${part}-${index}`}>{index > 0 ? " 路 " : ""}{part}</span>;
    })}</span>;
  }
  if (!value.startsWith("AlphaOsu!")) return value;
  return <span className="alpha-meta">{value.split(" 路 ").map((part, index) => <span className={part.startsWith("预测PP") || part.startsWith("PP潜力") ? "alpha-pp" : ""} key={`${part}-${index}`}>{index > 0 ? " 路 " : ""}{part}</span>)}</span>;
}
function mirrorNameFromUrl(url: string) { if (url.includes("osu.ppy.sh/osu/")) return "osu! official"; if (url.includes("hinamizawa")) return "Hinamizawa"; if (url.includes("catboy.best")) return "Catboy"; if (url.includes("nerinyan")) return "Nerinyan"; if (url.includes("sayobot")) return "Sayobot"; return "未知"; }
function downloadModeLabel(value: string) { if (value === "osu") return "仅 .osu"; if (value === "noVideo") return "不带视频"; return "带视频"; }
function tabTitle(tab: AppTab) { const map = { settings: "设置", search: "搜图", downloads: "下载任务", playlists: "歌单" }; return map[tab]; }
function isTaskFinished(task: DownloadTask) { return task.status === "completed" || task.status === "staged"; }
function getOverallProgress(tasks: DownloadTask[], progress: Record<string, DownloadGroupProgress>) {
  const groups = groupDownloadTasks(tasks, progress);
  const total = groups.reduce((sum, group) => sum + group.total, 0);
  const completed = groups.reduce((sum, group) => sum + group.completed, 0);
  const downloadedBytes = groups.reduce((sum, group) => sum + group.downloadedBytes, 0);
  const percent = total ? Math.floor((completed / total) * 100) : 0;
  return { total, completed, percent, downloadedBytes, isActiveUnknown: false };
}
function groupDownloadTasks(tasks: DownloadTask[], progress: Record<string, DownloadGroupProgress>) {
  const map = new Map<string, DownloadTask[]>();
  for (const task of tasks) {
    const id = task.groupId || `legacy-${task.createdAt}`;
    map.set(id, [...(map.get(id) || []), task]);
  }
  const ids = new Set([...Object.keys(progress || {}), ...map.keys()]);
  return [...ids].map((id) => {
    const groupTasks = map.get(id) || [];
    const first = groupTasks[0];
    const summary = progress?.[id];
    const activeCompleted = groupTasks.filter(isTaskFinished).length;
    const total = Math.max(summary?.totalTasks || 0, groupTasks.length);
    const completed = Math.min(total, (summary?.completedTasks || 0) + activeCompleted);
    const downloadedBytes = (summary?.completedBytes || 0) + groupTasks.reduce((sum, task) => sum + task.downloadedBytes, 0);
    return {
      id,
      name: summary?.name || first?.groupName || `任务 ${id.slice(-6)}`,
      source: summary?.source || first?.groupSource || "旧下载队列",
      destination: summary?.destination || first?.groupDestination || "通常下载",
      tasks: groupTasks,
      total,
      completed,
      downloadedBytes,
      percent: total ? Math.floor((completed / total) * 100) : 0,
    };
  }).filter((group) => group.total > 0);
}
function normalizeTheme(value: unknown) { if (value === "lime" || value === "BFFF00+222222") return "lime"; if (value === "sky" || value === "89C2FF+E6E7FF") return "sky"; return "cyan"; }
function normalizeSettings<T extends { mirrorPriority?: unknown; mixedMode?: unknown; theme?: unknown; localSource?: unknown }>(settings: T): T & { mirrorPriority: string[]; mixedMode: boolean; theme: string; localSource: LocalSource } { return { ...settings, mixedMode: Boolean(settings.mixedMode), mirrorPriority: normalizeMirrorPriority(settings.mirrorPriority), theme: normalizeTheme(settings.theme), localSource: normalizeLocalSource(settings.localSource) }; }
function normalizeLocalSource(value: unknown): LocalSource { return value === "lazer" ? "lazer" : "stable"; }
function countLocalBySource(localBeatmapsets: Record<string, { detectedFrom?: string }>, localSource: LocalSource) { return Object.values(localBeatmapsets).filter((entry) => localSource === "lazer" ? entry.detectedFrom?.startsWith("lazer") : !entry.detectedFrom?.startsWith("lazer")).length; }
function normalizeMirrorPriority(value: unknown) { const input = Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []; const merged = [...input, ...defaultMirrorPriority]; return merged.filter((item, index) => defaultMirrorPriority.includes(item) && merged.indexOf(item) === index); }
function upsertTask(tasks: DownloadTask[], task: DownloadTask) { const index = tasks.findIndex((item) => item.id === task.id); if (index === -1) return [...tasks, { ...task }]; const next = [...tasks]; next[index] = { ...task }; return next; }
function statusText(status: DownloadTask["status"]) { const map = { pending: "待开始", queued: "排队中", downloading: "下载中", staged: "已缓存", paused: "暂停", failed: "失败", completed: "完成", cancelled: "取消" }; return map[status]; }
