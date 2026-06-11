use chrono::Utc;
use futures_util::StreamExt;
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{Mutex, Semaphore},
    time::timeout,
};

type SharedStore = Arc<Mutex<AppStore>>;
const MAX_QUEUE_TASKS: usize = 1000;
const APP_REFERER: &str = "https://github.com/linnzero00/Osu-Beatmap-Seekman";
const APP_USER_AGENT: &str = "OsuBeatmapSeekman/1.0.1 (+https://github.com/linnzero00/Osu-Beatmap-Seekman)";
const DOWNLOAD_STALL_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    songs_dir: String,
    osu_client_id: String,
    osu_client_secret: String,
    bearer_token: String,
    concurrent_downloads: usize,
    include_video: bool,
    download_mode: String,
    hide_existing: bool,
    mirror_priority: Vec<String>,
    mixed_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            songs_dir: String::new(),
            osu_client_id: String::new(),
            osu_client_secret: String::new(),
            bearer_token: String::new(),
            concurrent_downloads: 3,
            include_video: true,
            download_mode: "video".to_string(),
            hide_existing: false,
            mirror_priority: default_mirror_priority(),
            mixed_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AppStore {
    settings: Settings,
    local_beatmapsets: HashMap<String, LocalBeatmapset>,
    tasks: Vec<DownloadTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalBeatmapset {
    beatmapset_id: u64,
    folder_path: String,
    detected_from: String,
    scanned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadTask {
    id: String,
    beatmapset_id: u64,
    title: String,
    artist: String,
    #[serde(default = "default_true")]
    include_video: bool,
    #[serde(default = "default_download_mode")]
    download_mode: String,
    beatmap_id: Option<u64>,
    url: String,
    target_path: String,
    temp_path: String,
    total_bytes: Option<u64>,
    downloaded_bytes: u64,
    #[serde(default)]
    retry_generation: u64,
    status: String,
    error: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Filters {
    query: Option<String>,
    status: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    min_stars: Option<String>,
    max_stars: Option<String>,
    min_od: Option<String>,
    max_od: Option<String>,
    min_hp: Option<String>,
    max_hp: Option<String>,
    min_cs: Option<String>,
    max_cs: Option<String>,
    min_ar: Option<String>,
    max_ar: Option<String>,
    min_bpm: Option<String>,
    max_bpm: Option<String>,
    min_length: Option<String>,
    max_length: Option<String>,
    mode: Option<String>,
    key_count: Option<String>,
    max_pages: Option<String>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeatmapsetItem {
    id: u64,
    title: String,
    artist: String,
    creator: String,
    ranked_date: String,
    status: String,
    modes: Vec<String>,
    min_stars: Option<f64>,
    max_stars: Option<f64>,
    min_od: Option<f64>,
    max_od: Option<f64>,
    min_hp: Option<f64>,
    max_hp: Option<f64>,
    min_cs: Option<f64>,
    max_cs: Option<f64>,
    min_ar: Option<f64>,
    max_ar: Option<f64>,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    #[serde(default)]
    key_counts: Vec<u8>,
    #[serde(default)]
    beatmap_ids: Vec<u64>,
    playcount: u64,
    favourite_count: u64,
    exists_local: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    count: usize,
    local_beatmapsets: HashMap<String, LocalBeatmapset>,
}

#[derive(Debug, Serialize, Clone)]
struct DownloadEvent {
    #[serde(rename = "type")]
    kind: String,
    tasks: Option<Vec<DownloadTask>>,
    task: Option<DownloadTask>,
    error: Option<String>,
}

struct RuntimeState {
    store: SharedStore,
    client: Client,
    token_cache: Mutex<Option<TokenCache>>,
    paused: Arc<Mutex<bool>>,
    queue_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone)]
struct TokenCache {
    token: String,
    expires_at_ms: i64,
}

#[tauri::command]
async fn get_state(state: State<'_, RuntimeState>) -> Result<AppStore, String> {
    Ok(state.store.lock().await.clone())
}

#[tauri::command]
async fn save_settings(settings: Value, app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Settings, String> {
    let mut store = state.store.lock().await;
    merge_settings(&mut store.settings, settings);
    save_store(&app, &store).await?;
    *state.token_cache.lock().await = None;
    Ok(store.settings.clone())
}

#[tauri::command]
async fn select_songs_dir(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Option<String>, String> {
    let folder = tokio::task::spawn_blocking(|| rfd::FileDialog::new().set_title("Select osu! Songs folder").pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = folder else {
        return Ok(None);
    };
    let dir = path.to_string_lossy().to_string();
    let mut store = state.store.lock().await;
    store.settings.songs_dir = dir.clone();
    save_store(&app, &store).await?;
    Ok(Some(dir))
}

#[tauri::command]
async fn scan_songs(songs_dir: Option<String>, app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<ScanResult, String> {
    let dir = {
        let store = state.store.lock().await;
        songs_dir.filter(|v| !v.is_empty()).unwrap_or_else(|| store.settings.songs_dir.clone())
    };
    if dir.is_empty() {
        return Err("Please select the Songs folder first.".to_string());
    }
    let local = scan_songs_directory(Path::new(&dir)).await?;
    let mut store = state.store.lock().await;
    store.local_beatmapsets = local.clone();
    save_store(&app, &store).await?;
    Ok(ScanResult { count: local.len(), local_beatmapsets: local })
}

#[tauri::command]
async fn search_beatmapsets(filters: Filters, state: State<'_, RuntimeState>) -> Result<Vec<BeatmapsetItem>, String> {
    let token = get_api_token(&state).await?;
    let mut items = search_osu(&state.client, &token, &filters).await?;
    let local_ids: HashSet<String> = state.store.lock().await.local_beatmapsets.keys().cloned().collect();
    for item in &mut items {
        item.exists_local = Some(local_ids.contains(&item.id.to_string()));
    }
    Ok(items)
}

#[tauri::command]
async fn enqueue_downloads(items: Vec<BeatmapsetItem>, app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Vec<DownloadTask>, String> {
    let now = Utc::now().to_rfc3339();
    let mut store = state.store.lock().await;
    if store.settings.songs_dir.is_empty() {
        return Err("Please select the Songs folder first.".to_string());
    }
    let existing: HashSet<String> = store.tasks.iter().filter(|t| t.status != "cancelled").map(task_dedupe_key).collect();
    let songs_dir = PathBuf::from(&store.settings.songs_dir);
    let osu_files_dir = app_sibling_osu_dir();
    let settings = store.settings.clone();
    let download_mode = normalize_download_mode(&settings.download_mode, settings.include_video);
    let cache_dir = download_cache_dir();
    for item in items {
        if download_mode == "osu" {
            for beatmap_id in &item.beatmap_ids {
                if store.tasks.len() >= MAX_QUEUE_TASKS {
                    break;
                }
                let key = format!("osu:{beatmap_id}");
                if existing.contains(&key) {
                    continue;
                }
                let name = sanitize_file_name(&format!("{} {} - {}.osu", beatmap_id, item.artist, item.title));
                let target = osu_files_dir.join(name);
                let id_suffix: String = rand::thread_rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();
                let cache_file = cache_dir.join(format!("{}-{}.osu.part", beatmap_id, id_suffix));
                store.tasks.push(DownloadTask {
                    id: format!("osu-{}-{}-{}", beatmap_id, Utc::now().timestamp_millis(), id_suffix),
                    beatmapset_id: item.id,
                    title: item.title.clone(),
                    artist: item.artist.clone(),
                    include_video: false,
                    download_mode: "osu".to_string(),
                    beatmap_id: Some(*beatmap_id),
                    url: format!("https://osu.ppy.sh/osu/{beatmap_id}"),
                    target_path: target.to_string_lossy().to_string(),
                    temp_path: cache_file.to_string_lossy().to_string(),
                    total_bytes: None,
                    downloaded_bytes: 0,
                    retry_generation: 0,
                    status: "pending".to_string(),
                    error: String::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
            continue;
        }
        let key = format!("osz:{}", item.id);
        if existing.contains(&key) {
            continue;
        }
        if store.tasks.len() >= MAX_QUEUE_TASKS {
            break;
        }
        let include_video = download_mode == "video";
        let name = sanitize_file_name(&format!("{} {} - {}.osz", item.id, item.artist, item.title));
        let target = songs_dir.join(name);
        let id_suffix: String = rand::thread_rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();
        let cache_file = cache_dir.join(format!("{}-{}.osz.part", item.id, id_suffix));
        store.tasks.push(DownloadTask {
            id: format!("{}-{}-{}", item.id, Utc::now().timestamp_millis(), id_suffix),
            beatmapset_id: item.id,
            title: item.title,
            artist: item.artist,
            include_video,
            download_mode: download_mode.clone(),
            beatmap_id: None,
            url: mirror_candidates_for_settings(item.id, include_video, &settings)
                .first()
                .map(|candidate| candidate.url.clone())
                .unwrap_or_default(),
            target_path: target.to_string_lossy().to_string(),
            temp_path: cache_file.to_string_lossy().to_string(),
            total_bytes: None,
            downloaded_bytes: 0,
            retry_generation: 0,
            status: "pending".to_string(),
            error: String::new(),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }
    save_store(&app, &store).await?;
    emit_tasks(&app, &store.tasks)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn start_downloads(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Value, String> {
    *state.paused.lock().await = false;
    {
        let mut store = state.store.lock().await;
        for task in &mut store.tasks {
            if task.status == "pending" {
                task.status = "queued".to_string();
                task.updated_at = Utc::now().to_rfc3339();
            }
        }
        save_store(&app, &store).await?;
        emit_tasks(&app, &store.tasks)?;
    }
    let app_handle = app.clone();
    let state_inner = RuntimeStateHandle::from_state(&state);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_queue(app_handle.clone(), state_inner).await {
            let _ = app_handle.emit("downloads:event", DownloadEvent {
                kind: "error".to_string(),
                tasks: None,
                task: None,
                error: Some(error),
            });
        }
    });
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn pause_downloads(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Value, String> {
    *state.paused.lock().await = true;
    let mut store = state.store.lock().await;
    for task in &mut store.tasks {
        if task.status == "downloading" {
            task.status = "paused".to_string();
            task.updated_at = Utc::now().to_rfc3339();
        }
    }
    save_store(&app, &store).await?;
    emit_tasks(&app, &store.tasks)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn clear_completed(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Vec<DownloadTask>, String> {
    let mut store = state.store.lock().await;
    store.tasks.retain(|task| task.status != "completed");
    save_store(&app, &store).await?;
    emit_tasks(&app, &store.tasks)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn retry_failed_downloads(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Vec<DownloadTask>, String> {
    *state.paused.lock().await = false;
    let (tasks, temp_paths) = {
        let mut store = state.store.lock().await;
        let settings = store.settings.clone();
        let mut temp_paths = Vec::new();
        for task in &mut store.tasks {
            if task.status != "completed" && task.status != "cancelled" {
                temp_paths.push(task.temp_path.clone());
                task.status = "queued".to_string();
                task.error.clear();
                task.total_bytes = None;
                task.downloaded_bytes = 0;
                task.retry_generation = task.retry_generation.saturating_add(1);
                task.url = first_download_url(task, &settings);
                task.temp_path = fresh_temp_path(task).to_string_lossy().to_string();
                task.updated_at = Utc::now().to_rfc3339();
            }
        }
        save_store(&app, &store).await?;
        emit_tasks(&app, &store.tasks)?;
        (store.tasks.clone(), temp_paths)
    };
    for temp_path in temp_paths {
        let _ = fs::remove_file(temp_path).await;
    }
    Ok(tasks)
}

#[tauri::command]
async fn clear_all_downloads(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<Vec<DownloadTask>, String> {
    *state.paused.lock().await = true;
    let mut store = state.store.lock().await;
    store.tasks.clear();
    save_store(&app, &store).await?;
    emit_tasks(&app, &store.tasks)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn open_api_page() -> Result<Value, String> {
    open_url("https://osu.ppy.sh/home/account/edit#authenticator-app")?;
    Ok(serde_json::json!({ "ok": true }))
}

#[derive(Clone)]
struct RuntimeStateHandle {
    store: SharedStore,
    client: Client,
    paused: Arc<Mutex<bool>>,
    queue_lock: Arc<Mutex<()>>,
}

impl RuntimeStateHandle {
    fn from_state(state: &State<'_, RuntimeState>) -> Self {
        Self {
            store: state.store.clone(),
            client: state.client.clone(),
            paused: state.paused.clone(),
            queue_lock: state.queue_lock.clone(),
        }
    }
}

async fn run_queue(app: tauri::AppHandle, state: RuntimeStateHandle) -> Result<(), String> {
    let _guard = state.queue_lock.lock().await;
    let limit = { state.store.lock().await.settings.concurrent_downloads.clamp(1, 8) };
    let semaphore = Arc::new(Semaphore::new(limit));

    loop {
        if *state.paused.lock().await {
            break;
        }
        let task_ids = {
            let store = state.store.lock().await;
            store.tasks
                .iter()
                .filter(|task| matches!(task.status.as_str(), "queued" | "paused" | "failed"))
                .map(|task| task.id.clone())
                .collect::<Vec<_>>()
        };
        if task_ids.is_empty() {
            break;
        }
        let mut handles = Vec::new();
        for task_id in task_ids {
            let permit = semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
            let app_handle = app.clone();
            let state_handle = state.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let _ = download_task(app_handle, state_handle, task_id).await;
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    }
    Ok(())
}

async fn download_task(app: tauri::AppHandle, state: RuntimeStateHandle, task_id: String) -> Result<(), String> {
    let mut task = {
        let mut store = state.store.lock().await;
        let task = store.tasks.iter_mut().find(|task| task.id == task_id).ok_or("Task not found")?;
        task.status = "downloading".to_string();
        task.error.clear();
        task.updated_at = Utc::now().to_rfc3339();
        task.clone()
    };
    let retry_generation = task.retry_generation;
    persist_and_emit(&app, &state.store).await?;

    if let Some(parent) = Path::new(&task.temp_path).parent() {
        fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let settings = {
        let store = state.store.lock().await;
        store.settings.clone()
    };
    let candidates = download_candidates_for_task(&task, &settings);
    let mut errors = Vec::new();
    'mirrors: for candidate in candidates {
        if !is_attempt_current(&state.store, &task_id, retry_generation).await {
            return Ok(());
        }
        update_task_attempt(&app, &state.store, &task_id, retry_generation, &candidate.url, &format!("trying mirror: {}", candidate.label)).await?;
    let mut start = fs::metadata(&task.temp_path).await.map(|m| m.len()).unwrap_or(0);
    task.downloaded_bytes = start;

    let mut request = state
        .client
        .get(&candidate.url)
        .header(header::REFERER, APP_REFERER)
        .header(header::USER_AGENT, APP_USER_AGENT);
    if start > 0 {
        request = request.header(header::RANGE, format!("bytes={}-", start));
    }
        let response = match timeout(Duration::from_secs(30), request.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            errors.push(format!("{}: {}", candidate.label, error));
            update_task_attempt(&app, &state.store, &task_id, retry_generation, &candidate.url, &format!("{} failed, trying next mirror", candidate.label)).await?;
            continue 'mirrors;
        }
        Err(_) => {
            errors.push(format!("{}: response timeout", candidate.label));
            update_task_attempt(&app, &state.store, &task_id, retry_generation, &candidate.url, &format!("{} failed, trying next mirror", candidate.label)).await?;
            continue 'mirrors;
        }
    };
    if !is_attempt_current(&state.store, &task_id, retry_generation).await {
        return Ok(());
    }
    if start > 0 && response.status().as_u16() == 200 {
        start = 0;
        task.downloaded_bytes = 0;
        let _ = fs::remove_file(&task.temp_path).await;
    }
    if !(response.status().is_success() || response.status().as_u16() == 206) {
        errors.push(format!("{}: HTTP {}", candidate.label, response.status()));
        update_task_attempt(&app, &state.store, &task_id, retry_generation, &candidate.url, &format!("{} failed, trying next mirror", candidate.label)).await?;
        continue 'mirrors;
    }
    if let Some(length) = response.content_length() {
        task.total_bytes = Some(if response.status().as_u16() == 206 { start + length } else { length });
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(start > 0)
        .write(true)
        .truncate(start == 0)
        .open(&task.temp_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    loop {
        if *state.paused.lock().await {
            mark_paused(&app, &state.store, &task_id, retry_generation).await?;
            return Ok(());
        }
        if !is_attempt_current(&state.store, &task_id, retry_generation).await {
            return Ok(());
        }
        let Some(chunk) = (match timeout(Duration::from_secs(DOWNLOAD_STALL_TIMEOUT_SECS), stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                errors.push(format!("{}: stalled for {} seconds", candidate.label, DOWNLOAD_STALL_TIMEOUT_SECS));
                drop(file);
                reset_stalled_attempt(&app, &state.store, &task_id, retry_generation, &task.temp_path, &format!("{} stalled, switching mirror", candidate.label)).await?;
                continue 'mirrors;
            }
        }) else {
            break;
        };
        let bytes = match chunk {
            Ok(bytes) => bytes,
            Err(error) => {
                errors.push(format!("{}: {}", candidate.label, error));
                drop(file);
                reset_stalled_attempt(&app, &state.store, &task_id, retry_generation, &task.temp_path, &format!("{} failed, switching mirror", candidate.label)).await?;
                continue 'mirrors;
            }
        };
        if !is_attempt_current(&state.store, &task_id, retry_generation).await {
            return Ok(());
        }
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
        task.downloaded_bytes += bytes.len() as u64;
        update_progress(&app, &state.store, &task_id, retry_generation, task.downloaded_bytes, task.total_bytes).await?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    if !is_attempt_current(&state.store, &task_id, retry_generation).await {
        return Ok(());
    }
    if let Some(parent) = Path::new(&task.target_path).parent() {
        fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    fs::rename(&task.temp_path, &task.target_path).await.map_err(|e| e.to_string())?;
    mark_completed(&app, &state.store, &task_id, retry_generation).await?;
    return Ok(());
    }
    mark_failed(&app, &state.store, &task_id, retry_generation, &format!("all mirrors failed: {}", errors.join("; "))).await?;
    Ok(())
}

async fn scan_songs_directory(songs_dir: &Path) -> Result<HashMap<String, LocalBeatmapset>, String> {
    let mut local = HashMap::new();
    let mut entries = fs::read_dir(songs_dir).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let file_type = entry.file_type().await.map_err(|e| e.to_string())?;
        let folder_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_file() {
            if folder_path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("osz")) {
                if let Some(id) = leading_number(&name) {
                    local.insert(id.to_string(), local_entry(id, &folder_path, "osz-file"));
                }
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        if let Some(id) = leading_number(&name) {
            local.insert(id.to_string(), local_entry(id, &folder_path, "folder"));
            continue;
        }
        if let Some(id) = find_beatmapset_id_in_folder(&folder_path).await {
            local.insert(id.to_string(), local_entry(id, &folder_path, "osu-file"));
        }
    }
    Ok(local)
}

async fn find_beatmapset_id_in_folder(folder: &Path) -> Option<u64> {
    let mut entries = fs::read_dir(folder).await.ok()?;
    while let Some(entry) = entries.next_entry().await.ok()? {
        if !entry.file_type().await.ok()?.is_file() {
            continue;
        }
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("osu") {
            continue;
        }
        let raw = fs::read_to_string(entry.path()).await.ok()?;
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("BeatmapSetID:") {
                if let Ok(id) = value.trim().parse::<u64>() {
                    if id > 0 {
                        return Some(id);
                    }
                }
            }
        }
    }
    None
}

async fn search_osu(client: &Client, token: &str, filters: &Filters) -> Result<Vec<BeatmapsetItem>, String> {
    let max_pages = filters.max_pages.as_deref().unwrap_or("10").parse::<usize>().unwrap_or(10).clamp(1, 50);
    let mut cursor = String::new();
    let mut results = Vec::new();
    for _ in 0..max_pages {
        let status = filters.status.as_deref().unwrap_or("ranked");
        let status = if status == "loved" { "loved" } else { "ranked" };
        let sort = api_sort(filters);
        let mut query = vec![("s", status.to_string()), ("sort", sort.to_string())];
        if let Some(mode) = filters.mode.as_deref().and_then(mode_query_value) {
            query.push(("m", mode.to_string()));
        }
        let search_query = build_search_query(filters);
        if !search_query.is_empty() {
            query.push(("q", search_query));
        }
        if !cursor.is_empty() {
            query.push(("cursor_string", cursor.clone()));
        }
        let response = client
            .get("https://osu.ppy.sh/api/v2/beatmapsets/search")
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Beatmapset search failed: HTTP {}", response.status()));
        }
        let data: Value = response.json().await.map_err(|e| e.to_string())?;
        if let Some(sets) = data.get("beatmapsets").and_then(|v| v.as_array()) {
            for set in sets {
                let item = map_beatmapset(set, filters);
                if matches_filters(&item, filters) {
                    results.push(item);
                }
            }
        }
        cursor = data.get("cursor_string").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if cursor.is_empty() {
            break;
        }
    }
    let mut seen = HashSet::new();
    results.retain(|item| seen.insert(item.id));
    sort_results(&mut results, filters);
    Ok(results)
}

async fn get_api_token(state: &State<'_, RuntimeState>) -> Result<String, String> {
    let settings = state.store.lock().await.settings.clone();
    if !settings.bearer_token.trim().is_empty() {
        return Ok(settings.bearer_token.trim().to_string());
    }
    if let Some(cache) = state.token_cache.lock().await.clone() {
        if cache.expires_at_ms > Utc::now().timestamp_millis() + 60_000 {
            return Ok(cache.token);
        }
    }
    if settings.osu_client_id.trim().is_empty() || settings.osu_client_secret.trim().is_empty() {
        return Err("Please fill osu! OAuth Client ID and Client Secret, or paste a Bearer Token.".to_string());
    }
    let response = state
        .client
        .post("https://osu.ppy.sh/oauth/token")
        .json(&serde_json::json!({
            "client_id": settings.osu_client_id.trim().parse::<u64>().map_err(|_| "Client ID must be numeric")?,
            "client_secret": settings.osu_client_secret,
            "grant_type": "client_credentials",
            "scope": "public"
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("osu! OAuth failed: HTTP {}", response.status()));
    }
    let data: Value = response.json().await.map_err(|e| e.to_string())?;
    let token = data.get("access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let expires_in = data.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    *state.token_cache.lock().await = Some(TokenCache {
        token: token.clone(),
        expires_at_ms: Utc::now().timestamp_millis() + expires_in * 1000,
    });
    Ok(token)
}

fn map_beatmapset(set: &Value, filters: &Filters) -> BeatmapsetItem {
    let beatmaps = set.get("beatmaps").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut stars = Vec::new();
    let mut ods = Vec::new();
    let mut hps = Vec::new();
    let mut css = Vec::new();
    let mut ars = Vec::new();
    let mut bpms = Vec::new();
    let mut lengths = Vec::new();
    let mut modes = HashSet::new();
    let mut key_counts = HashSet::new();
    let mut beatmap_ids = Vec::new();
    for beatmap in beatmaps {
        if !beatmap_matches_mode_and_key(&beatmap, filters) {
            continue;
        }
        if let Some(id) = beatmap.get("id").and_then(|v| v.as_u64()) {
            beatmap_ids.push(id);
        }
        if let Some(value) = beatmap.get("difficulty_rating").and_then(|v| v.as_f64()) {
            stars.push(value);
        }
        if let Some(value) = beatmap.get("accuracy").and_then(|v| v.as_f64()) {
            ods.push(value);
        }
        if let Some(value) = beatmap.get("drain").and_then(|v| v.as_f64()) {
            hps.push(value);
        }
        if let Some(value) = beatmap.get("cs").and_then(|v| v.as_f64()) {
            css.push(value);
        }
        if let Some(value) = beatmap.get("ar").and_then(|v| v.as_f64()) {
            ars.push(value);
        }
        if let Some(value) = beatmap.get("bpm").and_then(|v| v.as_f64()).or_else(|| set.get("bpm").and_then(|v| v.as_f64())) {
            bpms.push(value);
        }
        if let Some(value) = beatmap.get("total_length").or_else(|| beatmap.get("hit_length")).and_then(|v| v.as_u64()) {
            lengths.push(value);
        }
        if let Some(value) = beatmap.get("mode").and_then(|v| v.as_str()) {
            modes.insert(value.to_string());
        }
        if beatmap.get("mode").and_then(|v| v.as_str()) == Some("mania") {
            if let Some(keys) = beatmap.get("cs").and_then(|v| v.as_f64()).map(|v| v.round() as u8) {
                if keys > 0 {
                    key_counts.insert(keys);
                }
            }
        }
    }
    let mut key_counts = key_counts.into_iter().collect::<Vec<_>>();
    key_counts.sort_unstable();
    BeatmapsetItem {
        id: set.get("id").and_then(|v| v.as_u64()).unwrap_or_default(),
        title: string_field(set, "title"),
        artist: string_field(set, "artist"),
        creator: string_field(set, "creator"),
        ranked_date: string_field(set, "ranked_date").if_empty(string_field(set, "approved_date")),
        status: string_field(set, "status"),
        modes: sorted_strings(modes),
        min_stars: stars.iter().copied().reduce(f64::min),
        max_stars: stars.iter().copied().reduce(f64::max),
        min_od: ods.iter().copied().reduce(f64::min),
        max_od: ods.iter().copied().reduce(f64::max),
        min_hp: hps.iter().copied().reduce(f64::min),
        max_hp: hps.iter().copied().reduce(f64::max),
        min_cs: css.iter().copied().reduce(f64::min),
        max_cs: css.iter().copied().reduce(f64::max),
        min_ar: ars.iter().copied().reduce(f64::min),
        max_ar: ars.iter().copied().reduce(f64::max),
        min_bpm: bpms.iter().copied().reduce(f64::min),
        max_bpm: bpms.iter().copied().reduce(f64::max),
        min_length: lengths.iter().copied().min(),
        max_length: lengths.iter().copied().max(),
        key_counts,
        beatmap_ids,
        playcount: set.get("play_count").and_then(|v| v.as_u64()).unwrap_or_default(),
        favourite_count: set.get("favourite_count").and_then(|v| v.as_u64()).unwrap_or_default(),
        exists_local: Some(false),
    }
}

fn matches_filters(item: &BeatmapsetItem, filters: &Filters) -> bool {
    if item.modes.is_empty() {
        return false;
    }
    if let Some(mode) = filters.mode.as_deref().filter(|mode| *mode != "any") {
        if !item.modes.iter().any(|item_mode| item_mode == mode) {
            return false;
        }
    }
    if let Some(from) = filters.date_from.as_deref().filter(|v| !v.is_empty()) {
        if !item.ranked_date.is_empty() && item.ranked_date[..10.min(item.ranked_date.len())] < *from {
            return false;
        }
    }
    if let Some(to) = filters.date_to.as_deref().filter(|v| !v.is_empty()) {
        if !item.ranked_date.is_empty() && item.ranked_date[..10.min(item.ranked_date.len())] > *to {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_stars.as_deref()) {
        if item.max_stars.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_stars.as_deref()) {
        if item.min_stars.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_od.as_deref()) {
        if item.max_od.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_od.as_deref()) {
        if item.min_od.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_hp.as_deref()) {
        if item.max_hp.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_hp.as_deref()) {
        if item.min_hp.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_cs.as_deref()) {
        if item.max_cs.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_cs.as_deref()) {
        if item.min_cs.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_ar.as_deref()) {
        if item.max_ar.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_ar.as_deref()) {
        if item.min_ar.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_f64(filters.min_bpm.as_deref()) {
        if item.max_bpm.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_f64(filters.max_bpm.as_deref()) {
        if item.min_bpm.is_some_and(|v| v > max) {
            return false;
        }
    }
    if let Some(min) = parse_u64(filters.min_length.as_deref()) {
        if item.max_length.is_some_and(|v| v < min) {
            return false;
        }
    }
    if let Some(max) = parse_u64(filters.max_length.as_deref()) {
        if item.min_length.is_some_and(|v| v > max) {
            return false;
        }
    }
    if filters.mode.as_deref() == Some("mania") {
        if let Some(keys) = parse_u8(filters.key_count.as_deref()) {
            if !item.key_counts.contains(&keys) {
                return false;
            }
        }
    }
    true
}

fn build_search_query(filters: &Filters) -> String {
    let mut parts = Vec::new();
    if let Some(q) = filters.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        parts.push(q.to_string());
    }
    if let Some(from) = filters.date_from.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("ranked>={from}"));
    }
    if let Some(to) = filters.date_to.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("ranked<={to}"));
    }
    if let Some(min) = range_min_for_query(filters.min_stars.as_deref(), 3.0) {
        parts.push(format!("stars>={min:.1}"));
    }
    if let Some(max) = range_max_for_query(filters.max_stars.as_deref(), 7.0) {
        parts.push(format!("stars<={max:.1}"));
    }
    if let Some(min) = range_min_for_query(filters.min_od.as_deref(), 0.0) {
        parts.push(format!("od>={min:.1}"));
    }
    if let Some(max) = range_max_for_query(filters.max_od.as_deref(), 10.0) {
        parts.push(format!("od<={max:.1}"));
    }
    if let Some(min) = range_min_for_query(filters.min_hp.as_deref(), 0.0) {
        parts.push(format!("hp>={min:.1}"));
    }
    if let Some(max) = range_max_for_query(filters.max_hp.as_deref(), 10.0) {
        parts.push(format!("hp<={max:.1}"));
    }
    if let Some(min) = range_min_for_query(filters.min_cs.as_deref(), 0.0) {
        parts.push(format!("cs>={min:.1}"));
    }
    if let Some(max) = range_max_for_query(filters.max_cs.as_deref(), 10.0) {
        parts.push(format!("cs<={max:.1}"));
    }
    if let Some(min) = range_min_for_query(filters.min_ar.as_deref(), 0.0) {
        parts.push(format!("ar>={min:.1}"));
    }
    if let Some(max) = range_max_for_query(filters.max_ar.as_deref(), 10.0) {
        parts.push(format!("ar<={max:.1}"));
    }
    if let Some(min) = range_min_for_query(filters.min_bpm.as_deref(), 0.0) {
        parts.push(format!("bpm>={min:.0}"));
    }
    if let Some(max) = range_max_for_query(filters.max_bpm.as_deref(), 400.0) {
        parts.push(format!("bpm<={max:.0}"));
    }
    if filters.mode.as_deref() == Some("mania") {
        if let Some(keys) = parse_u8(filters.key_count.as_deref()) {
            parts.push(format!("key={keys}"));
        }
    }
    parts.join(" ")
}

fn range_min_for_query(value: Option<&str>, default: f64) -> Option<f64> {
    parse_f64(value).filter(|v| (v - default).abs() > f64::EPSILON)
}

fn range_max_for_query(value: Option<&str>, default: f64) -> Option<f64> {
    parse_f64(value).filter(|v| (v - default).abs() > f64::EPSILON)
}

fn api_sort(filters: &Filters) -> &'static str {
    match (filters.sort_by.as_deref(), filters.sort_dir.as_deref()) {
        (Some("time") | None, Some("asc")) => "ranked_asc",
        (Some("time") | None, _) => "ranked_desc",
        _ => "ranked_desc",
    }
}

fn sort_results(items: &mut [BeatmapsetItem], filters: &Filters) {
    let ascending = filters.sort_dir.as_deref() == Some("asc");
    match filters.sort_by.as_deref().unwrap_or("time") {
        "length" => sort_by_optional_u64(items, ascending, |item| item.max_length),
        "bpm" => sort_by_optional_f64(items, ascending, |item| item.max_bpm),
        _ => {
            items.sort_by(|a, b| a.ranked_date.cmp(&b.ranked_date));
            if !ascending {
                items.reverse();
            }
        }
    }
}

fn sort_by_optional_u64(items: &mut [BeatmapsetItem], ascending: bool, get: fn(&BeatmapsetItem) -> Option<u64>) {
    items.sort_by(|a, b| get(a).unwrap_or_default().cmp(&get(b).unwrap_or_default()));
    if !ascending {
        items.reverse();
    }
}

fn sort_by_optional_f64(items: &mut [BeatmapsetItem], ascending: bool, get: fn(&BeatmapsetItem) -> Option<f64>) {
    items.sort_by(|a, b| get(a).unwrap_or_default().total_cmp(&get(b).unwrap_or_default()));
    if !ascending {
        items.reverse();
    }
}

fn beatmap_matches_mode_and_key(beatmap: &Value, filters: &Filters) -> bool {
    if let Some(mode) = filters.mode.as_deref().filter(|mode| *mode != "any") {
        if beatmap.get("mode").and_then(|v| v.as_str()) != Some(mode) {
            return false;
        }
    }
    if filters.mode.as_deref() == Some("mania") {
        if let Some(keys) = parse_u8(filters.key_count.as_deref()) {
            let beatmap_keys = beatmap.get("cs").and_then(|v| v.as_f64()).map(|v| v.round() as u8);
            if beatmap_keys != Some(keys) {
                return false;
            }
        }
    }
    true
}

fn mode_query_value(mode: &str) -> Option<&'static str> {
    match mode {
        "osu" => Some("0"),
        "taiko" => Some("1"),
        "fruits" => Some("2"),
        "mania" => Some("3"),
        _ => None,
    }
}

fn sorted_strings(values: HashSet<String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    values
}

async fn load_store(app: &tauri::AppHandle) -> AppStore {
    let path = store_path(app);
    match fs::read_to_string(path).await {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppStore::default(),
    }
}

async fn save_store(app: &tauri::AppHandle, store: &AppStore) -> Result<(), String> {
    let path = store_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(path, raw).await.map_err(|e| e.to_string())
}

fn store_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("state.json")
}

async fn persist_and_emit(app: &tauri::AppHandle, store: &SharedStore) -> Result<(), String> {
    let store = store.lock().await;
    save_store(app, &store).await?;
    emit_tasks(app, &store.tasks)
}

fn emit_tasks(app: &tauri::AppHandle, tasks: &[DownloadTask]) -> Result<(), String> {
    app.emit("downloads:event", DownloadEvent {
        kind: "tasks".to_string(),
        tasks: Some(tasks.to_vec()),
        task: None,
        error: None,
    })
    .map_err(|e| e.to_string())
}

async fn is_attempt_current(store: &SharedStore, id: &str, retry_generation: u64) -> bool {
    let store = store.lock().await;
    store
        .tasks
        .iter()
        .find(|task| task.id == id)
        .is_some_and(|task| task.retry_generation == retry_generation && task.status != "cancelled")
}

async fn update_progress(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64, downloaded: u64, total: Option<u64>) -> Result<(), String> {
    let (task, tasks) = {
        let mut store = store.lock().await;
        let Some(task) = store.tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(());
        };
        if task.retry_generation != retry_generation {
            return Ok(());
        }
        task.downloaded_bytes = downloaded;
        task.total_bytes = total;
        task.updated_at = Utc::now().to_rfc3339();
        (task.clone(), store.tasks.clone())
    };
    app.emit("downloads:event", DownloadEvent {
        kind: "progress".to_string(),
        tasks: Some(tasks),
        task: Some(task),
        error: None,
    })
    .map_err(|e| e.to_string())
}

async fn update_task_attempt(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64, url: &str, error: &str) -> Result<(), String> {
    let task = {
        let mut store = store.lock().await;
        let Some(task) = store.tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(());
        };
        if task.retry_generation != retry_generation {
            return Ok(());
        }
        task.url = url.to_string();
        task.error = error.to_string();
        task.status = "downloading".to_string();
        task.updated_at = Utc::now().to_rfc3339();
        task.clone()
    };
    app.emit("downloads:event", DownloadEvent {
        kind: "progress".to_string(),
        tasks: None,
        task: Some(task),
        error: None,
    })
    .map_err(|e| e.to_string())
}

async fn reset_stalled_attempt(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64, temp_path: &str, error: &str) -> Result<(), String> {
    let (task, tasks) = {
        let mut store = store.lock().await;
        let Some(task) = store.tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(());
        };
        if task.retry_generation != retry_generation {
            return Ok(());
        }
        task.downloaded_bytes = 0;
        task.total_bytes = None;
        task.error = error.to_string();
        task.updated_at = Utc::now().to_rfc3339();
        (task.clone(), store.tasks.clone())
    };
    let _ = fs::remove_file(temp_path).await;
    app.emit("downloads:event", DownloadEvent {
        kind: "progress".to_string(),
        tasks: Some(tasks),
        task: Some(task),
        error: None,
    })
    .map_err(|e| e.to_string())
}

async fn mark_paused(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "paused", "")
        .await
}

async fn mark_failed(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64, error: &str) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "failed", error).await
}

async fn mark_completed(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "completed", "").await
}

async fn set_status(app: &tauri::AppHandle, store: &SharedStore, id: &str, retry_generation: u64, status: &str, error: &str) -> Result<(), String> {
    let mut data = store.lock().await;
    let completed_info = if let Some(index) = data.tasks.iter().position(|task| task.id == id) {
        if data.tasks[index].retry_generation != retry_generation {
            return Ok(());
        }
        if status == "completed" {
            let task = data.tasks.remove(index);
            Some((task.beatmapset_id, task.target_path))
        } else {
            let task = &mut data.tasks[index];
            task.status = status.to_string();
            task.error = error.to_string();
            task.updated_at = Utc::now().to_rfc3339();
            None
        }
    } else {
        None
    };
    if let Some((beatmapset_id, target_path)) = completed_info {
        data.local_beatmapsets.insert(beatmapset_id.to_string(), LocalBeatmapset {
            beatmapset_id,
            folder_path: target_path,
            detected_from: "download".to_string(),
            scanned_at: Utc::now().to_rfc3339(),
        });
    }
    save_store(app, &data).await?;
    emit_tasks(app, &data.tasks)
}

fn merge_settings(settings: &mut Settings, value: Value) {
    if let Some(v) = value.get("songsDir").and_then(|v| v.as_str()) {
        settings.songs_dir = v.to_string();
    }
    if let Some(v) = value.get("osuClientId").and_then(|v| v.as_str()) {
        settings.osu_client_id = v.to_string();
    }
    if let Some(v) = value.get("osuClientSecret").and_then(|v| v.as_str()) {
        settings.osu_client_secret = v.to_string();
    }
    if let Some(v) = value.get("bearerToken").and_then(|v| v.as_str()) {
        settings.bearer_token = v.to_string();
    }
    if let Some(v) = value.get("concurrentDownloads").and_then(|v| v.as_u64()) {
        settings.concurrent_downloads = v as usize;
    }
    if let Some(v) = value.get("includeVideo").and_then(|v| v.as_bool()) {
        settings.include_video = v;
        if settings.download_mode != "osu" {
            settings.download_mode = if v { "video".to_string() } else { "noVideo".to_string() };
        }
    }
    if let Some(v) = value.get("downloadMode").and_then(|v| v.as_str()) {
        settings.download_mode = normalize_download_mode(v, settings.include_video);
        settings.include_video = settings.download_mode == "video";
    }
    if let Some(v) = value.get("hideExisting").and_then(|v| v.as_bool()) {
        settings.hide_existing = v;
    }
    if let Some(v) = value.get("mixedMode").and_then(|v| v.as_bool()) {
        settings.mixed_mode = v;
    }
    if let Some(values) = value.get("mirrorPriority").and_then(|v| v.as_array()) {
        let mut priority = Vec::new();
        for value in values {
            if let Some(key) = value.as_str().and_then(normalize_mirror_key) {
                if !priority.iter().any(|item| item == key) {
                    priority.push(key.to_string());
                }
            }
        }
        if !priority.is_empty() {
            settings.mirror_priority = priority;
        }
    }
}

fn local_entry(id: u64, path: &Path, detected_from: &str) -> LocalBeatmapset {
    LocalBeatmapset {
        beatmapset_id: id,
        folder_path: path.to_string_lossy().to_string(),
        detected_from: detected_from.to_string(),
        scanned_at: Utc::now().to_rfc3339(),
    }
}

fn leading_number(value: &str) -> Option<u64> {
    let digits: String = value.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse::<u64>().ok().filter(|id| *id > 0)
}

#[derive(Debug, Clone)]
struct MirrorCandidate {
    label: &'static str,
    url: String,
}

fn default_mirror_priority() -> Vec<String> {
    ["hinamizawa", "catboy", "nerinyan", "sayobot"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

fn default_download_mode() -> String {
    "video".to_string()
}

fn normalize_download_mode(value: &str, include_video: bool) -> String {
    match value {
        "osu" => "osu".to_string(),
        "noVideo" | "no_video" | "novideo" => "noVideo".to_string(),
        "video" => "video".to_string(),
        _ if include_video => "video".to_string(),
        _ => "noVideo".to_string(),
    }
}

fn download_cache_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("download-cache")
}

fn fresh_temp_path(task: &DownloadTask) -> PathBuf {
    let id_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    if task.download_mode == "osu" {
        let beatmap_id = task.beatmap_id.unwrap_or(task.beatmapset_id);
        download_cache_dir().join(format!("{}-{}.osu.part", beatmap_id, id_suffix))
    } else {
        download_cache_dir().join(format!("{}-{}.osz.part", task.beatmapset_id, id_suffix))
    }
}

fn app_sibling_osu_dir() -> PathBuf {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    root.parent()
        .map(|parent| parent.join(".osu"))
        .unwrap_or_else(|| root.join(".osu"))
}

fn task_dedupe_key(task: &DownloadTask) -> String {
    if task.download_mode == "osu" {
        format!("osu:{}", task.beatmap_id.unwrap_or_default())
    } else {
        format!("osz:{}", task.beatmapset_id)
    }
}

fn first_download_url(task: &DownloadTask, settings: &Settings) -> String {
    download_candidates_for_task(task, settings)
        .first()
        .map(|candidate| candidate.url.clone())
        .unwrap_or_default()
}

fn download_candidates_for_task(task: &DownloadTask, settings: &Settings) -> Vec<MirrorCandidate> {
    if task.download_mode == "osu" {
        let Some(beatmap_id) = task.beatmap_id else {
            return Vec::new();
        };
        return vec![MirrorCandidate {
            label: "osu! official",
            url: format!("https://osu.ppy.sh/osu/{beatmap_id}"),
        }];
    }
    mirror_candidates_for_settings(task.beatmapset_id, task.include_video, settings)
}

fn mirror_candidates_for_settings(id: u64, include_video: bool, settings: &Settings) -> Vec<MirrorCandidate> {
    if settings.mixed_mode {
        let defaults = default_mirror_priority();
        let offset = (id as usize) % defaults.len();
        let priority = defaults
            .iter()
            .cycle()
            .skip(offset)
            .take(defaults.len())
            .cloned()
            .collect::<Vec<_>>();
        mirror_candidates(id, include_video, &priority)
    } else {
        mirror_candidates(id, include_video, &settings.mirror_priority)
    }
}

fn mirror_candidates(id: u64, include_video: bool, priority: &[String]) -> Vec<MirrorCandidate> {
    let mut keys = Vec::new();
    for value in priority {
        if let Some(key) = normalize_mirror_key(value) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    for key in default_mirror_priority() {
        let key = normalize_mirror_key(&key).unwrap();
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.into_iter()
        .map(|key| MirrorCandidate {
            label: mirror_label(key),
            url: mirror_url(key, id, include_video),
        })
        .collect()
}

fn normalize_mirror_key(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hinamizawa" | "hinai" => Some("hinamizawa"),
        "catboy" | "catboy.best" => Some("catboy"),
        "nerinyan" | "nerinyan.moe" => Some("nerinyan"),
        "sayobot" | "sayo" => Some("sayobot"),
        _ => None,
    }
}

fn mirror_label(key: &str) -> &'static str {
    match key {
        "hinamizawa" => "Hinamizawa",
        "catboy" => "Catboy",
        "nerinyan" => "Nerinyan",
        "sayobot" => "Sayobot",
        _ => "Mirror",
    }
}

fn mirror_url(key: &str, id: u64, include_video: bool) -> String {
    match key {
        "hinamizawa" => {
            let base = format!("https://mirror.hinamizawa.ai/api/v1/hinai/d/{id}");
            if include_video { base } else { format!("{base}?noVideo=1") }
        }
        "catboy" => {
            if include_video {
                format!("https://catboy.best/d/{id}")
            } else {
                format!("https://catboy.best/d/{id}n")
            }
        }
        "nerinyan" => {
            let base = format!("https://api.nerinyan.moe/d/{id}");
            if include_video { base } else { format!("{base}?noVideo=1") }
        }
        "sayobot" => {
            if include_video {
                format!("https://txy1.sayobot.cn/beatmaps/download/full/{id}")
            } else {
                format!("https://txy1.sayobot.cn/beatmaps/download/novideo/{id}")
            }
        }
        _ => format!("https://mirror.hinamizawa.ai/api/v1/hinai/d/{id}"),
    }
}

fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|ch| if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() { '_' } else { ch })
        .take(180)
        .collect()
}

fn parse_f64(value: Option<&str>) -> Option<f64> {
    value.and_then(|v| if v.trim().is_empty() { None } else { v.trim().parse().ok() })
}

fn parse_u64(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| if v.trim().is_empty() { None } else { v.trim().parse().ok() })
}

fn parse_u8(value: Option<&str>) -> Option<u8> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() || trimmed == "any" {
            None
        } else {
            trimmed.parse().ok()
        }
    })
}

fn default_true() -> bool {
    true
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_http_client() -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::REFERER, header::HeaderValue::from_static(APP_REFERER));
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static(APP_USER_AGENT));
    Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()
        .expect("failed to create HTTP client")
}

fn string_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

trait IfEmpty {
    fn if_empty(self, fallback: String) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: String) -> String {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let store = tauri::async_runtime::block_on(load_store(&app_handle));
            app.manage(RuntimeState {
                store: Arc::new(Mutex::new(store)),
                client: build_http_client(),
                token_cache: Mutex::new(None),
                paused: Arc::new(Mutex::new(true)),
                queue_lock: Arc::new(Mutex::new(())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            select_songs_dir,
            scan_songs,
            search_beatmapsets,
            enqueue_downloads,
            start_downloads,
            pause_downloads,
            clear_completed,
            retry_failed_downloads,
            clear_all_downloads,
            open_api_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
