import { CalendarDays, Download, FolderOpen, Gauge, Pause, Play, RotateCcw, Search, Settings } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";

type Mode = "any" | "osu" | "taiko" | "fruits" | "mania";

const defaultMirrorPriority = ["hinamizawa", "catboy", "nerinyan", "sayobot"];
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

export function App() {
  const [settings, setSettings] = useState({
    songsDir: "", osuClientId: "", osuClientSecret: "", bearerToken: "", concurrentDownloads: 3,
    includeVideo: true, downloadMode: "video", hideExisting: false, mirrorPriority: defaultMirrorPriority, mixedMode: false,
  });
  const [filters, setFilters] = useState(defaultFilters);
  const [items, setItems] = useState<BeatmapsetItem[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [localCount, setLocalCount] = useState(0);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    api.getState().then((state) => {
      setSettings((prev) => normalizeSettings({ ...prev, ...state.settings }));
      setTasks(state.tasks || []);
      setLocalCount(Object.keys(state.localBeatmapsets || {}).length);
    }).catch((error) => setMessage(String(error)));
    return api.onDownloadEvent((event) => {
      const type = event.kind ?? event.type;
      if (type === "tasks") setTasks([...(event.tasks || [])]);
      if (type === "progress" && event.task) {
        if (event.tasks) setTasks([...(event.tasks || [])]);
        else setTasks((current) => upsertTask(current, event.task));
      }
      if (type === "error") setMessage(event.error);
    });
  }, []);

  useEffect(() => {
    if (!tasks.length) return;
    const timer = window.setInterval(() => api.getState().then((state) => setTasks(state.tasks || [])).catch(() => undefined), 500);
    return () => window.clearInterval(timer);
  }, [tasks.length]);

  const availableItems = useMemo(() => items.filter((item) => !item.existsLocal), [items]);
  const visibleItems = settings.hideExisting ? availableItems : items;
  const selectedItems = useMemo(() => availableItems.filter((item) => selectedIds.has(item.id)), [availableItems, selectedIds]);
  const selectedDownloaded = tasks.reduce((sum, task) => sum + task.downloadedBytes, 0);
  const overall = getOverallProgress(tasks);

  async function saveSettings(patch = settings) {
    const saved = await api.saveSettings(patch);
    setSettings((prev) => normalizeSettings({ ...prev, ...saved }));
  }

  async function selectSongsDir() {
    const dir = await api.selectSongsDir();
    if (dir) { setSettings((prev) => ({ ...prev, songsDir: dir })); setMessage("已选择 Songs 文件夹。"); }
  }

  async function scanSongs() {
    runBusy("正在扫描本地曲库...", async () => {
      const result = await api.scanSongs(settings.songsDir);
      setLocalCount(result.count);
      setMessage(`扫描完成：识别到 ${result.count} 个本地 beatmapset。`);
    });
  }

  async function search() {
    runBusy("正在构建下图列表...", async () => {
      await saveSettings();
      const result = await api.searchBeatmapsets(filters);
      setItems(result);
      setSelectedIds(new Set(result.filter((item) => !item.existsLocal).map((item) => item.id)));
      setMessage(`列表构建完成：${result.length} 个结果，${result.filter((item) => item.existsLocal).length} 个已在本地。`);
    });
  }

  async function enqueue() {
    runBusy("正在加入下载队列...", async () => {
      await saveSettings();
      const nextTasks = await api.enqueueDownloads(selectedItems);
      setTasks(nextTasks);
      setMessage(`已加入 ${selectedItems.length} 个任务，等待手动开始；下载选项：${downloadModeLabel(settings.downloadMode)}。`);
    });
  }

  async function runBusy(label: string, fn: () => Promise<void>) {
    try { setBusy(label); setMessage(""); await fn(); }
    catch (error) { setMessage(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(""); }
  }

  function updateSetting(key: string, value: unknown) { setSettings((prev) => ({ ...prev, [key]: value })); }
  function updateDownloadMode(value: string) {
    setSettings((prev) => ({ ...prev, downloadMode: value, includeVideo: value === "video" }));
  }
  function updateFilter(key: string, value: string) { setFilters((prev) => ({ ...prev, [key]: value })); }
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
    const nextTasks = await api.retryFailedDownloads();
    setTasks(nextTasks);
    if (nextTasks.length) await api.startDownloads();
    setMessage("已丢弃旧断点，并按当前镜像策略重新开始。");
  }
  async function startQueue() { await api.startDownloads(); setMessage("下载队列已开始。"); }
  async function pauseQueue() { await api.pauseDownloads(); const state = await api.getState(); setTasks(state.tasks || []); setMessage("下载队列已暂停。"); }
  async function clearAllDownloads() { const nextTasks = await api.clearAllDownloads(); setTasks(nextTasks); setMessage("下载队列已清空。"); }
  function toggleItem(id: number) { setSelectedIds((current) => { const next = new Set(current); next.has(id) ? next.delete(id) : next.add(id); return next; }); }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark">o!</div><div><h1>Osu! Beatmap Seekman</h1><p>osu! beatmapset 批量下载器</p></div></div>
        <section className="panel">
          <h2><FolderOpen size={17} /> 目录</h2>
          <button className="primary" onClick={selectSongsDir}><FolderOpen size={16} /> 选择 Songs</button>
          <div className="path-box">{settings.songsDir || "尚未选择"}</div>
          <button className="ghost" onClick={scanSongs} disabled={!settings.songsDir || Boolean(busy)}><RotateCcw size={16} /> 扫描本地曲库</button>
          <div className="metric"><span>本地已识别</span><strong>{localCount}</strong></div>
          <label className="check-row"><input type="checkbox" checked={settings.hideExisting} onChange={(e) => updateSetting("hideExisting", e.target.checked)} /><span>隐藏已有图</span></label>
        </section>
        <section className="panel">
          <h2><Settings size={17} /> osu! API</h2>
          <label>Client ID<input value={settings.osuClientId} onChange={(e) => updateSetting("osuClientId", e.target.value)} /></label>
          <label>Client Secret<input type="password" value={settings.osuClientSecret} onChange={(e) => updateSetting("osuClientSecret", e.target.value)} /></label>
          <label>Bearer Token<input type="password" value={settings.bearerToken} onChange={(e) => updateSetting("bearerToken", e.target.value)} /></label>
          <label>并发下载<input type="number" min={1} max={8} value={settings.concurrentDownloads} onChange={(e) => updateSetting("concurrentDownloads", Number(e.target.value))} /></label>
          <button className="ghost" onClick={() => saveSettings().then(() => setMessage("设置已保存。"))}>保存设置</button>
        </section>
        <section className="panel">
          <h2><Download size={17} /> 镜像策略</h2>
          <label className="check-row"><input type="checkbox" checked={settings.mixedMode} onChange={(e) => updateSetting("mixedMode", e.target.checked)} /><span>混杂模式：四源轮流并发</span></label>
          <div className="mirror-list">
            {normalizeMirrorPriority(settings.mirrorPriority).map((mirror, index) => (
              <div className="mirror-row" key={mirror}><span>{index + 1}. {mirrorLabels[mirror]}</span><div><button type="button" onClick={() => moveMirror(index, -1)} disabled={index === 0}>↑</button><button type="button" onClick={() => moveMirror(index, 1)} disabled={index === defaultMirrorPriority.length - 1}>↓</button></div></div>
            ))}
          </div>
        </section>
      </aside>
      <section className="workspace">
        <header className="toolbar"><div><h2>筛选与下载</h2><p>{busy || message || "选择条件后构建列表，加入队列后需要手动点击开始下载。"}</p></div></header>
        <section className="filters">
          <div className="filter-row filter-row-primary">
            <label className="filter-query"><Search size={15} /> 关键词<input value={filters.query} onChange={(e) => updateFilter("query", e.target.value)} placeholder="artist / title / mapper" /></label>
            <label>状态<select value={filters.status} onChange={(e) => updateFilter("status", e.target.value)}><option value="ranked">Ranked</option><option value="loved">Loved</option></select></label>
            <label>模式<select value={filters.mode} onChange={(e) => updateFilter("mode", e.target.value)}><option value="any">全部</option><option value="osu">osu</option><option value="taiko">taiko</option><option value="fruits">fruits</option><option value="mania">mania</option></select></label>
            <label>页数<input value={filters.maxPages} onChange={(e) => updateFilter("maxPages", e.target.value)} /></label>
            <label>排序<select value={filters.sortBy} onChange={(e) => updateFilter("sortBy", e.target.value)}><option value="time">时间</option><option value="length">时长</option><option value="bpm">BPM</option></select></label>
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
        </section>
        <div className="actions"><button className="primary" onClick={search} disabled={Boolean(busy)}><Search size={16} /> 构建列表</button><label className="inline-select">下载版本<select value={settings.downloadMode} onChange={(e) => updateDownloadMode(e.target.value)}><option value="video">带视频 .osz</option><option value="noVideo">不带视频 .osz</option><option value="osu">仅 .osu 文件</option></select></label><button onClick={enqueue} disabled={!selectedItems.length || Boolean(busy)}><Download size={16} /> 加入队列</button><span>{selectedItems.length} 个待加入，当前任务已下载 {formatBytes(selectedDownloaded)}</span></div>
        <section className="content-grid">
          <div className="table-panel"><div className="table-head"><strong>候选列表</strong><button onClick={() => setSelectedIds(new Set(availableItems.map((item) => item.id)))}>全选可下载</button></div><div className="table">{visibleItems.map((item) => <label className={`row ${item.existsLocal ? "muted" : ""}`} key={item.id}><input type="checkbox" checked={selectedIds.has(item.id)} disabled={item.existsLocal} onChange={() => toggleItem(item.id)} /><div className="main-cell"><strong>{item.artist} - {item.title}</strong><span>#{item.id} · {item.status} · {item.creator} · {formatDate(item.rankedDate)} · {item.modes.join(", ")}{item.keyCounts.length ? ` · ${item.keyCounts.join("/")}K` : ""}</span></div><div>{formatStars(item)}</div><div>{formatOdHp(item)}</div><div>{formatCsArBpm(item)}</div><div>{formatLength(item)}</div><div>{item.existsLocal ? "已存在" : "可下载"}</div></label>)}{!visibleItems.length && <div className="empty">还没有列表。</div>}</div></div>
          <div className="queue-panel"><div className="queue-summary"><strong>下载队列</strong><span>{overall.completed}/{overall.total} · 已下载 {formatBytes(overall.downloadedBytes)}</span></div><div className="queue-actions queue-actions-row"><button className="primary" onClick={startQueue} disabled={!tasks.length}><Play size={16} /> 开始</button><button onClick={pauseQueue} disabled={!tasks.some((task) => task.status === "downloading")}><Pause size={16} /> 暂停</button><button onClick={retryFailedDownloads} disabled={!tasks.length}>一键重试</button><button onClick={clearAllDownloads} disabled={!tasks.length}>清除所有</button></div><div className={`overall-bar ${overall.isActiveUnknown ? "indeterminate" : ""}`}><div style={{ width: `${overall.percent}%` }} /></div><div className="queue-list">{tasks.map((task) => { const percent = task.totalBytes ? Math.floor((task.downloadedBytes / task.totalBytes) * 100) : 0; const isUnknownActive = !task.totalBytes && task.status === "downloading"; const visiblePercent = task.downloadedBytes > 0 && task.status === "downloading" ? Math.max(percent, 2) : percent; return <div className="task" key={task.id}><div className="task-title"><strong>{task.artist} - {task.title}</strong><span>{statusText(task.status)}</span></div><div className={`task-bar ${isUnknownActive ? "indeterminate" : ""}`}><div style={{ width: `${isUnknownActive ? 100 : Math.min(visiblePercent, 100)}%` }} /></div><div className="task-meta">已下载 {formatBytes(task.downloadedBytes)}{task.totalBytes ? ` / ${formatBytes(task.totalBytes)}` : ""} · 源 {mirrorNameFromUrl(task.url)} · {downloadModeLabel(task.downloadMode)}{task.beatmapId ? ` · #${task.beatmapId}` : ""}{task.error && ` · ${task.error}`}</div></div>; })}{!tasks.length && <div className="empty">队列为空。</div>}</div></div>
        </section>
      </section>
    </main>
  );
}

function RangeSlider({ label, min, max, step, valueMin, valueMax, onChange }: { label: string; min: number; max: number; step: number; valueMin: number; valueMax: number; onChange: (min: number, max: number) => void; }) {
  const low = clamp(valueMin, min, max); const high = clamp(valueMax, min, max); const left = ((low - min) / (max - min)) * 100; const right = 100 - ((high - min) / (max - min)) * 100;
  return <div className="range-card"><div className="range-head"><span>{label}</span><strong>{low.toFixed(1)} - {high.toFixed(1)}</strong></div><div className="dual-range" style={{ "--range-left": `${left}%`, "--range-right": `${right}%` } as React.CSSProperties}><input type="range" min={min} max={max} step={step} value={low} onChange={(e) => onChange(Math.min(Number(e.target.value), high), high)} /><input type="range" min={min} max={max} step={step} value={high} onChange={(e) => onChange(low, Math.max(Number(e.target.value), low))} /></div></div>;
}
function clamp(value: number, min: number, max: number) { return Math.min(Math.max(value, min), max); }
function formatBytes(value: number | null | undefined) { if (!value) return "0 MB"; const mb = value / 1024 / 1024; return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(1)} MB`; }
function formatDate(value: string) { return value ? value.slice(0, 10) : "未知日期"; }
function formatStars(item: BeatmapsetItem) { return item.minStars === null || item.maxStars === null ? "未知星数" : `${item.minStars.toFixed(2)}-${item.maxStars.toFixed(2)}*`; }
function formatOdHp(item: BeatmapsetItem) { return item.minOd === null || item.maxOd === null || item.minHp === null || item.maxHp === null ? "OD/HP 未知" : `OD ${item.minOd.toFixed(1)}-${item.maxOd.toFixed(1)} · HP ${item.minHp.toFixed(1)}-${item.maxHp.toFixed(1)}`; }
function formatCsArBpm(item: BeatmapsetItem) { const cs = item.minCs !== null && item.maxCs !== null ? `CS ${item.minCs.toFixed(1)}-${item.maxCs.toFixed(1)}` : "CS ?"; const ar = item.minAr !== null && item.maxAr !== null ? `AR ${item.minAr.toFixed(1)}-${item.maxAr.toFixed(1)}` : "AR ?"; const bpm = item.minBpm !== null && item.maxBpm !== null ? `BPM ${Math.round(item.minBpm)}-${Math.round(item.maxBpm)}` : "BPM ?"; return `${cs} · ${ar} · ${bpm}`; }
function formatLength(item: BeatmapsetItem) { const seconds = item.maxLength || item.minLength; if (!seconds) return "未知长度"; return `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`; }
function mirrorNameFromUrl(url: string) { if (url.includes("osu.ppy.sh/osu/")) return "osu! official"; if (url.includes("hinamizawa")) return "Hinamizawa"; if (url.includes("catboy.best")) return "Catboy"; if (url.includes("nerinyan")) return "Nerinyan"; if (url.includes("sayobot")) return "Sayobot"; return "未知"; }
function downloadModeLabel(value: string) { if (value === "osu") return "仅 .osu"; if (value === "noVideo") return "不带视频"; return "带视频"; }
function getOverallProgress(tasks: DownloadTask[]) { const total = tasks.length; const completed = tasks.filter((task) => task.status === "completed").length; const totalBytes = tasks.reduce((sum, task) => sum + (task.totalBytes || 0), 0); const downloadedBytes = tasks.reduce((sum, task) => sum + task.downloadedBytes, 0); const percent = totalBytes ? Math.floor((downloadedBytes / totalBytes) * 100) : total ? Math.floor((completed / total) * 100) : 0; const isActiveUnknown = !totalBytes && tasks.some((task) => task.status === "downloading" && !task.totalBytes); return { total, completed, percent: isActiveUnknown ? 100 : percent, downloadedBytes, isActiveUnknown }; }
function normalizeSettings<T extends { mirrorPriority?: unknown; mixedMode?: unknown }>(settings: T): T & { mirrorPriority: string[]; mixedMode: boolean } { return { ...settings, mixedMode: Boolean(settings.mixedMode), mirrorPriority: normalizeMirrorPriority(settings.mirrorPriority) }; }
function normalizeMirrorPriority(value: unknown) { const input = Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []; const merged = [...input, ...defaultMirrorPriority]; return merged.filter((item, index) => defaultMirrorPriority.includes(item) && merged.indexOf(item) === index); }
function upsertTask(tasks: DownloadTask[], task: DownloadTask) { const index = tasks.findIndex((item) => item.id === task.id); if (index === -1) return [...tasks, { ...task }]; const next = [...tasks]; next[index] = { ...task }; return next; }
function statusText(status: DownloadTask["status"]) { const map = { pending: "待开始", queued: "排队中", downloading: "下载中", paused: "暂停", failed: "失败", completed: "完成", cancelled: "取消" }; return map[status]; }
