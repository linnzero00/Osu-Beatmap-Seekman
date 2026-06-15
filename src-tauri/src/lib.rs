use chrono::Utc;
use futures_util::StreamExt;
use md5::{Digest, Md5};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
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
const MAX_QUEUE_TASKS: usize = 1_000_000;
const APP_REFERER: &str = "https://github.com/linnzero00/Osu-Beatmap-Seekman";
const APP_USER_AGENT: &str = concat!(
    "OsuBeatmapSeekman/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/linnzero00/Osu-Beatmap-Seekman)"
);
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/linnzero00/Osu-Beatmap-Seekman/releases/latest";
const DOWNLOAD_STALL_TIMEOUT_SECS: u64 = 30;
const LAZER_MAX_BEATMAP_BYTES: u64 = 768 * 1024;
const LAZER_SCAN_READ_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    songs_dir: String,
    lazer_dir: String,
    stable_osu_dir: String,
    collection_auto_add: bool,
    collection_name: String,
    local_source: String,
    osu_client_id: String,
    osu_client_secret: String,
    bearer_token: String,
    concurrent_downloads: usize,
    include_video: bool,
    download_mode: String,
    hide_existing: bool,
    mirror_priority: Vec<String>,
    mixed_mode: bool,
    theme: String,
    dismissed_update_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            songs_dir: String::new(),
            lazer_dir: String::new(),
            stable_osu_dir: String::new(),
            collection_auto_add: false,
            collection_name: "Seekman Downloads".to_string(),
            local_source: "stable".to_string(),
            osu_client_id: String::new(),
            osu_client_secret: String::new(),
            bearer_token: String::new(),
            concurrent_downloads: 8,
            include_video: true,
            download_mode: "video".to_string(),
            hide_existing: false,
            mirror_priority: default_mirror_priority(),
            mixed_mode: false,
            theme: "cyan".to_string(),
            dismissed_update_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct AppStore {
    settings: Settings,
    local_beatmapsets: HashMap<String, LocalBeatmapset>,
    tasks: Vec<DownloadTask>,
    task_groups: HashMap<String, DownloadGroupProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct DownloadGroupProgress {
    id: String,
    name: String,
    source: String,
    destination: String,
    total_tasks: usize,
    completed_tasks: usize,
    completed_bytes: u64,
    created_at: String,
    updated_at: String,
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
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    group_source: String,
    #[serde(default)]
    group_destination: String,
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
    #[serde(default)]
    collection_beatmap_ids: Vec<u64>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlphaRecommendRequest {
    username: String,
    limit: Option<String>,
    mode: Option<String>,
    key_count: Option<String>,
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
    #[serde(default)]
    collection_beatmap_ids: Vec<u64>,
    #[serde(default)]
    source_collection: String,
    playcount: u64,
    favourite_count: u64,
    exists_local: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StableCollectionSummary {
    name: String,
    beatmap_count: usize,
    #[serde(default)]
    items: Vec<BeatmapsetItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistLocalApplyResult {
    applied_count: usize,
    applied_beatmapset_count: usize,
    missing_count: usize,
    missing_items: Vec<BeatmapsetItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    name: String,
    body: String,
    html_url: String,
    published_at: String,
    can_install_now: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
struct StableBeatmapInfo {
    beatmapset_id: u64,
    beatmap_id: u64,
    artist: String,
    title: String,
    creator: String,
    version: String,
    mode: String,
    md5: String,
    artist_unicode: String,
    title_unicode: String,
    audio_file_name: String,
    osu_file_name: String,
    ranked_status: u8,
    hitcircles: i16,
    sliders: i16,
    spinners: i16,
    last_modification_time: i64,
    ar: f32,
    cs: f32,
    hp: f32,
    od: f32,
    slider_velocity: f64,
    drain_time: i32,
    total_time: i32,
    preview_time: i32,
    bpm: Option<f64>,
    source: String,
    tags: String,
    folder_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    count: usize,
    local_beatmapsets: HashMap<String, LocalBeatmapset>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadEvent {
    #[serde(rename = "type")]
    kind: String,
    tasks: Option<Vec<DownloadTask>>,
    task_groups: Option<HashMap<String, DownloadGroupProgress>>,
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
async fn save_settings(
    settings: Value,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Settings, String> {
    let mut store = state.store.lock().await;
    merge_settings(&mut store.settings, settings);
    save_store(&app, &store).await?;
    *state.token_cache.lock().await = None;
    Ok(store.settings.clone())
}

#[tauri::command]
async fn select_songs_dir(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "android")]
    {
        let dir_path = ensure_android_songs_dir(&app).await?;
        let dir = dir_path.to_string_lossy().to_string();
        let mut store = state.store.lock().await;
        store.settings.songs_dir = dir.clone();
        save_store(&app, &store).await?;
        return Ok(Some(dir));
    }

    #[cfg(not(target_os = "android"))]
    {
        let folder = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Select osu! Songs folder")
                .pick_folder()
        })
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
}

#[tauri::command]
async fn select_lazer_dir(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "android")]
    {
        return Err("osu! lazer directory selection is only available on desktop.".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let folder = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Select osu! lazer folder")
                .pick_folder()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(path) = folder else {
            return Ok(None);
        };
        let dir = path.to_string_lossy().to_string();
        let mut store = state.store.lock().await;
        store.settings.lazer_dir = dir.clone();
        save_store(&app, &store).await?;
        Ok(Some(dir))
    }
}

#[tauri::command]
async fn select_stable_osu_dir(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "android")]
    {
        return Err("osu!stable collection editing is only available on desktop.".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let folder = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Select osu!stable folder")
                .pick_folder()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(path) = folder else {
            return Ok(None);
        };
        let dir = path.to_string_lossy().to_string();
        let mut store = state.store.lock().await;
        store.settings.stable_osu_dir = dir.clone();
        save_store(&app, &store).await?;
        Ok(Some(dir))
    }
}

#[tauri::command]
async fn scan_stable_collections(
    stable_osu_dir: Option<String>,
    state: State<'_, RuntimeState>,
) -> Result<Vec<StableCollectionSummary>, String> {
    let stable_dir = resolve_stable_osu_dir(stable_osu_dir, &state.store).await?;
    tokio::task::spawn_blocking(move || {
        let db_path = stable_dir.join("collection.db");
        let collection = read_collection_db(&db_path)?;
        let beatmaps = read_stable_osu_db(&stable_dir.join("osu!.db"))?;
        let by_md5 = beatmaps
            .into_iter()
            .map(|beatmap| (beatmap.md5.to_ascii_lowercase(), beatmap))
            .collect::<HashMap<_, _>>();
        Ok(collection
            .lists
            .into_iter()
            .map(|list| {
                let name = list.name;
                let items = list
                    .hashes
                    .iter()
                    .filter_map(|hash| by_md5.get(&hash.to_ascii_lowercase()))
                    .cloned()
                    .collect::<Vec<_>>();
                let mapped_items = stable_beatmaps_to_items(items, true, &name);
                StableCollectionSummary {
                    name,
                    beatmap_count: list.hashes.len(),
                    items: mapped_items,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_collection_playlist(
    stable_osu_dir: Option<String>,
    collection_name: String,
    selected_beatmap_ids: Option<Vec<u64>>,
    state: State<'_, RuntimeState>,
) -> Result<String, String> {
    let stable_dir = resolve_stable_osu_dir(stable_osu_dir, &state.store).await?;
    let name = non_empty_or_default(&collection_name, "Seekman Downloads");
    tokio::task::spawn_blocking(move || {
        export_collection_playlist_inner(&stable_dir, &name, selected_beatmap_ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn apply_local_playlist_items_to_collection(
    stable_osu_dir: Option<String>,
    collection_name: String,
    items: Vec<BeatmapsetItem>,
    commit: Option<bool>,
    state: State<'_, RuntimeState>,
) -> Result<PlaylistLocalApplyResult, String> {
    let stable_dir = resolve_stable_osu_dir(stable_osu_dir, &state.store).await?;
    let name = non_empty_or_default(&collection_name, "Seekman Downloads");
    let commit = commit.unwrap_or(true);
    tokio::task::spawn_blocking(move || {
        apply_local_playlist_items_to_collection_inner(&stable_dir, &name, items, commit)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_seekman_playlist(
    state: State<'_, RuntimeState>,
) -> Result<Vec<BeatmapsetItem>, String> {
    #[cfg(target_os = "android")]
    {
        let _ = state;
        return Err("歌单导入目前仅支持桌面端文件选择。".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let file = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Import Seekman playlist")
                .add_filter("CSV", &["csv"])
                .pick_file()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(path) = file else {
            return Ok(Vec::new());
        };
        let local_sets = {
            let store = state.store.lock().await;
            store.local_beatmapsets.clone()
        };
        tokio::task::spawn_blocking(move || import_seekman_playlist_inner(&path, &local_sets))
            .await
            .map_err(|e| e.to_string())?
    }
}

#[tauri::command]
async fn scan_songs(
    songs_dir: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<ScanResult, String> {
    let dir = {
        let store = state.store.lock().await;
        songs_dir
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| store.settings.songs_dir.clone())
    };
    if dir.is_empty() {
        return Err("Please select the Songs folder first.".to_string());
    }
    let local = scan_songs_directory(Path::new(&dir)).await?;
    let scan_count = local.len();
    let mut store = state.store.lock().await;
    store.settings.songs_dir = dir;
    store.settings.local_source = "stable".to_string();
    replace_local_source(&mut store.local_beatmapsets, local, |source| {
        matches!(source, "folder" | "osu-file" | "osz-file")
    });
    save_store(&app, &store).await?;
    Ok(ScanResult {
        count: scan_count,
        local_beatmapsets: store.local_beatmapsets.clone(),
    })
}

#[tauri::command]
async fn scan_lazer(
    lazer_dir: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<ScanResult, String> {
    let dir = {
        let store = state.store.lock().await;
        lazer_dir
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| store.settings.lazer_dir.clone())
    };
    if dir.is_empty() {
        return Err("Please select the osu! lazer folder first.".to_string());
    }
    let local = scan_lazer_directory(PathBuf::from(&dir)).await?;
    let scan_count = local.len();
    let mut store = state.store.lock().await;
    store.settings.lazer_dir = dir;
    store.settings.local_source = "lazer".to_string();
    replace_local_source(&mut store.local_beatmapsets, local, |source| {
        source.starts_with("lazer")
    });
    save_store(&app, &store).await?;
    Ok(ScanResult {
        count: scan_count,
        local_beatmapsets: store.local_beatmapsets.clone(),
    })
}

#[tauri::command]
async fn search_beatmapsets(
    filters: Filters,
    state: State<'_, RuntimeState>,
) -> Result<Vec<BeatmapsetItem>, String> {
    let token = get_api_token(&state).await?;
    let mut items = search_osu(&state.client, &token, &filters).await?;
    mark_existing_items(&mut items, &state).await;
    Ok(items)
}

#[tauri::command]
async fn search_alpha_recommendations(
    request: AlphaRecommendRequest,
    state: State<'_, RuntimeState>,
) -> Result<Vec<BeatmapsetItem>, String> {
    let mut items = search_alpha_osu(&state.client, &request).await?;
    mark_existing_items(&mut items, &state).await;
    Ok(items)
}

#[tauri::command]
async fn enqueue_downloads(
    items: Vec<BeatmapsetItem>,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    let now = Utc::now().to_rfc3339();
    let mut store = state.store.lock().await;
    ensure_mobile_songs_dir(&app, &mut store).await?;
    if store.settings.songs_dir.is_empty() {
        return Err("Please select the Songs folder first.".to_string());
    }
    let existing: HashSet<String> = store
        .tasks
        .iter()
        .filter(|t| t.status != "cancelled")
        .map(task_dedupe_key)
        .collect();
    let songs_dir = PathBuf::from(&store.settings.songs_dir);
    let osu_files_dir = app_sibling_osu_dir();
    let settings = store.settings.clone();
    let download_mode = normalize_download_mode(&settings.download_mode, settings.include_video);
    let cache_dir = download_cache_dir();
    let group_id = format!(
        "group-{}-{}",
        Utc::now().timestamp_millis(),
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect::<String>()
    );
    let group_source = group_source_from_items(&items);
    let group_destination = if settings.collection_auto_add && download_mode != "osu" {
        format!(
            "写入收藏夹：{}",
            non_empty_or_default(&settings.collection_name, "Seekman Downloads")
        )
    } else {
        "通常下载".to_string()
    };
    let group_name = format!("{} · {} 首", group_source, items.len());
    let initial_task_count = store.tasks.len();
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
                let name = sanitize_file_name(&format!(
                    "{} {} - {}.osu",
                    beatmap_id, item.artist, item.title
                ));
                let target = osu_files_dir.join(name);
                let id_suffix: String = rand::thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(8)
                    .map(char::from)
                    .collect();
                let cache_file = cache_dir.join(format!("{}-{}.osu.part", beatmap_id, id_suffix));
                store.tasks.push(DownloadTask {
                    id: format!(
                        "osu-{}-{}-{}",
                        beatmap_id,
                        Utc::now().timestamp_millis(),
                        id_suffix
                    ),
                    group_id: group_id.clone(),
                    group_name: group_name.clone(),
                    group_source: group_source.clone(),
                    group_destination: "仅 .osu 文件".to_string(),
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
                    collection_beatmap_ids: Vec::new(),
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
        let id_suffix: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();
        let cache_file = cache_dir.join(format!("{}-{}.osz.part", item.id, id_suffix));
        store.tasks.push(DownloadTask {
            id: format!(
                "{}-{}-{}",
                item.id,
                Utc::now().timestamp_millis(),
                id_suffix
            ),
            group_id: group_id.clone(),
            group_name: group_name.clone(),
            group_source: group_source.clone(),
            group_destination: group_destination.clone(),
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
            collection_beatmap_ids: item.collection_beatmap_ids,
            status: "pending".to_string(),
            error: String::new(),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }
    let added_task_count = store.tasks.len().saturating_sub(initial_task_count);
    if added_task_count > 0 {
        store.task_groups.insert(
            group_id.clone(),
            DownloadGroupProgress {
                id: group_id.clone(),
                name: group_name,
                source: group_source,
                destination: if download_mode == "osu" {
                    "仅 .osu 文件".to_string()
                } else {
                    group_destination
                },
                total_tasks: added_task_count,
                completed_tasks: 0,
                completed_bytes: 0,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        );
    }
    save_store(&app, &store).await?;
    emit_tasks(&app, &store)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn start_downloads(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
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
        emit_tasks(&app, &store)?;
    }
    let app_handle = app.clone();
    let state_inner = RuntimeStateHandle::from_state(&state);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_queue(app_handle.clone(), state_inner).await {
            let _ = app_handle.emit(
                "downloads:event",
                DownloadEvent {
                    kind: "error".to_string(),
                    tasks: None,
                    task_groups: None,
                    task: None,
                    error: Some(error),
                },
            );
        }
    });
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn pause_downloads(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
    *state.paused.lock().await = true;
    let mut store = state.store.lock().await;
    for task in &mut store.tasks {
        if task.status == "downloading" {
            task.status = "paused".to_string();
            task.updated_at = Utc::now().to_rfc3339();
        }
    }
    save_store(&app, &store).await?;
    emit_tasks(&app, &store)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn clear_completed(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    let mut store = state.store.lock().await;
    store.tasks.retain(|task| task.status != "completed");
    prune_empty_task_groups(&mut store);
    save_store(&app, &store).await?;
    emit_tasks(&app, &store)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn retry_failed_downloads(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    *state.paused.lock().await = false;
    let (tasks, temp_paths) = {
        let mut store = state.store.lock().await;
        let settings = store.settings.clone();
        let mut temp_paths = Vec::new();
        for task in &mut store.tasks {
            if task.status != "completed" && task.status != "cancelled" {
                temp_paths.push(task.temp_path.clone());
                recreate_retry_task(task, &settings);
            }
        }
        save_store(&app, &store).await?;
        emit_tasks(&app, &store)?;
        (store.tasks.clone(), temp_paths)
    };
    for temp_path in temp_paths {
        let _ = fs::remove_file(temp_path).await;
    }
    Ok(tasks)
}

#[tauri::command]
async fn clear_all_downloads(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    *state.paused.lock().await = true;
    let mut store = state.store.lock().await;
    store.tasks.clear();
    store.task_groups.clear();
    save_store(&app, &store).await?;
    emit_tasks(&app, &store)?;
    Ok(store.tasks.clone())
}

#[tauri::command]
async fn delete_download_group(
    group_id: String,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    let temp_paths = {
        let mut store = state.store.lock().await;
        let mut temp_paths = Vec::new();
        store.tasks.retain(|task| {
            if normalized_group_id(task) == group_id {
                temp_paths.push(task.temp_path.clone());
                false
            } else {
                true
            }
        });
        store.task_groups.remove(&group_id);
        save_store(&app, &store).await?;
        emit_tasks(&app, &store)?;
        temp_paths
    };
    for path in temp_paths {
        let _ = fs::remove_file(path).await;
    }
    Ok(state.store.lock().await.tasks.clone())
}

#[tauri::command]
async fn force_finish_download_group(
    group_id: String,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Vec<DownloadTask>, String> {
    finalize_staged_group(&app, &state.store, &group_id, true).await?;
    Ok(state.store.lock().await.tasks.clone())
}

#[tauri::command]
async fn open_api_page() -> Result<Value, String> {
    open_external_url("https://osu.ppy.sh/home/account/edit#authenticator-app".to_string()).await?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<Value, String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("只允许打开 http/https 链接。".to_string());
    }
    tauri_plugin_opener::open_url(trimmed, None::<&str>).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn check_for_updates(state: State<'_, RuntimeState>) -> Result<Option<UpdateInfo>, String> {
    let dismissed_version = {
        let store = state.store.lock().await;
        store.settings.dismissed_update_version.clone()
    };
    let release = fetch_latest_release(&state.client).await?;
    release_to_update_info(&release, &dismissed_version)
}

#[tauri::command]
async fn dismiss_update_version(
    version: String,
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Settings, String> {
    let mut store = state.store.lock().await;
    store.settings.dismissed_update_version = version;
    save_store(&app, &store).await?;
    Ok(store.settings.clone())
}

#[tauri::command]
async fn install_update_now(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<Value, String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        let _ = state;
        return Err("当前平台暂不支持应用内立即更新，请前往 GitHub Release 手动下载。".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let release = fetch_latest_release(&state.client).await?;
        if !is_release_newer(&release.tag_name) {
            return Err("当前已经是最新版本。".to_string());
        }
        let asset = find_windows_installer_asset(&release)
            .ok_or_else(|| "没有在最新 Release 中找到 Windows 安装包。".to_string())?;
        let file_name = sanitize_file_name(&asset.name);
        let update_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("updates");
        fs::create_dir_all(&update_dir)
            .await
            .map_err(|e| e.to_string())?;
        let target_path = update_dir.join(file_name);
        download_update_asset(&state.client, &asset.browser_download_url, &target_path).await?;
        tauri_plugin_opener::open_path(target_path.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "ok": true }))
    }
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
    let limit = {
        state
            .store
            .lock()
            .await
            .settings
            .concurrent_downloads
            .clamp(1, 64)
    };
    let semaphore = Arc::new(Semaphore::new(limit));

    loop {
        if *state.paused.lock().await {
            break;
        }
        let task_ids = {
            let store = state.store.lock().await;
            let next_group = store
                .tasks
                .iter()
                .filter(|task| matches!(task.status.as_str(), "queued" | "paused" | "failed"))
                .map(|task| normalized_group_id(task))
                .next();
            store
                .tasks
                .iter()
                .filter(|task| matches!(task.status.as_str(), "queued" | "paused" | "failed"))
                .filter(|task| next_group.as_deref() == Some(normalized_group_id(task).as_str()))
                .map(|task| task.id.clone())
                .collect::<Vec<_>>()
        };
        if task_ids.is_empty() {
            break;
        }
        let mut handles = Vec::new();
        for task_id in task_ids {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?;
            let app_handle = app.clone();
            let state_handle = state.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                let _permit = permit;
                if let Err(error) =
                    download_task(app_handle.clone(), state_handle.clone(), task_id.clone()).await
                {
                    let _ = mark_failed_latest(&app_handle, &state_handle.store, &task_id, &error)
                        .await;
                }
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    }
    Ok(())
}

async fn download_task(
    app: tauri::AppHandle,
    state: RuntimeStateHandle,
    task_id: String,
) -> Result<(), String> {
    let mut task = {
        let mut store = state.store.lock().await;
        let task = store
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or("Task not found")?;
        task.status = "downloading".to_string();
        task.error.clear();
        task.updated_at = Utc::now().to_rfc3339();
        task.clone()
    };
    let retry_generation = task.retry_generation;
    persist_and_emit(&app, &state.store).await?;
    prepare_runtime_temp_path(&app, &state.store, &task_id, retry_generation, &mut task).await?;

    if let Some(parent) = Path::new(&task.temp_path).parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
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
        update_task_attempt(
            &app,
            &state.store,
            &task_id,
            retry_generation,
            &candidate.url,
            &format!("{}: preparing request", candidate.label),
        )
        .await?;
        let mut start = fs::metadata(&task.temp_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        task.downloaded_bytes = start;

        let mut request = state
            .client
            .get(&candidate.url)
            .header(header::REFERER, APP_REFERER)
            .header(header::USER_AGENT, APP_USER_AGENT);
        if start > 0 {
            request = request.header(header::RANGE, format!("bytes={}-", start));
        }
        update_task_attempt(
            &app,
            &state.store,
            &task_id,
            retry_generation,
            &candidate.url,
            &format!("{}: sending request", candidate.label),
        )
        .await?;
        let response = match timeout(Duration::from_secs(30), request.send()).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                errors.push(format!("{}: {}", candidate.label, error));
                update_task_attempt(
                    &app,
                    &state.store,
                    &task_id,
                    retry_generation,
                    &candidate.url,
                    &format!("{} failed, trying next mirror", candidate.label),
                )
                .await?;
                continue 'mirrors;
            }
            Err(_) => {
                errors.push(format!("{}: response timeout", candidate.label));
                update_task_attempt(
                    &app,
                    &state.store,
                    &task_id,
                    retry_generation,
                    &candidate.url,
                    &format!("{} failed, trying next mirror", candidate.label),
                )
                .await?;
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
        update_task_attempt(
            &app,
            &state.store,
            &task_id,
            retry_generation,
            &candidate.url,
            &format!("{}: HTTP {}", candidate.label, response.status()),
        )
        .await?;
        if !(response.status().is_success() || response.status().as_u16() == 206) {
            errors.push(format!("{}: HTTP {}", candidate.label, response.status()));
            update_task_attempt(
                &app,
                &state.store,
                &task_id,
                retry_generation,
                &candidate.url,
                &format!("{} failed, trying next mirror", candidate.label),
            )
            .await?;
            continue 'mirrors;
        }
        if let Some(length) = response.content_length() {
            task.total_bytes = Some(if response.status().as_u16() == 206 {
                start + length
            } else {
                length
            });
        }
        update_task_attempt(
            &app,
            &state.store,
            &task_id,
            retry_generation,
            &candidate.url,
            &format!("{}: opening cache", candidate.label),
        )
        .await?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(start > 0)
            .write(true)
            .truncate(start == 0)
            .open(&task.temp_path)
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();
        update_task_attempt(
            &app,
            &state.store,
            &task_id,
            retry_generation,
            &candidate.url,
            &format!("{}: waiting for data", candidate.label),
        )
        .await?;
        loop {
            if *state.paused.lock().await {
                mark_paused(&app, &state.store, &task_id, retry_generation).await?;
                return Ok(());
            }
            if !is_attempt_current(&state.store, &task_id, retry_generation).await {
                return Ok(());
            }
            let Some(chunk) = (match timeout(
                Duration::from_secs(DOWNLOAD_STALL_TIMEOUT_SECS),
                stream.next(),
            )
            .await
            {
                Ok(chunk) => chunk,
                Err(_) => {
                    errors.push(format!(
                        "{}: stalled for {} seconds",
                        candidate.label, DOWNLOAD_STALL_TIMEOUT_SECS
                    ));
                    drop(file);
                    reset_stalled_attempt(
                        &app,
                        &state.store,
                        &task_id,
                        retry_generation,
                        &task.temp_path,
                        &format!("{} stalled, switching mirror", candidate.label),
                    )
                    .await?;
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
                    reset_stalled_attempt(
                        &app,
                        &state.store,
                        &task_id,
                        retry_generation,
                        &task.temp_path,
                        &format!("{} failed, switching mirror", candidate.label),
                    )
                    .await?;
                    continue 'mirrors;
                }
            };
            if !is_attempt_current(&state.store, &task_id, retry_generation).await {
                return Ok(());
            }
            file.write_all(&bytes)
                .await
                .map_err(|e| format!("write cache failed: {e}"))?;
            task.downloaded_bytes += bytes.len() as u64;
            update_progress(
                &app,
                &state.store,
                &task_id,
                retry_generation,
                task.downloaded_bytes,
                task.total_bytes,
            )
            .await?;
        }
        file.flush().await.map_err(|e| e.to_string())?;
        if !is_attempt_current(&state.store, &task_id, retry_generation).await {
            return Ok(());
        }
        if should_stage_playlist_group(&task) {
            stage_completed_download(&app, &state.store, &task_id, retry_generation, &mut task)
                .await?;
            if let Err(error) = try_finalize_staged_group(&app, &state.store, &task.group_id).await
            {
                let _ = app.emit(
                    "downloads:event",
                    DownloadEvent {
                        kind: "error".to_string(),
                        tasks: None,
                        task_groups: None,
                        task: None,
                        error: Some(format!("歌单任务提交失败：{error}")),
                    },
                );
            }
        } else {
            if let Some(parent) = Path::new(&task.target_path).parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            move_completed_file(&task.temp_path, &task.target_path).await?;
            if let Err(error) = add_download_to_collection_if_enabled(&state.store, &task).await {
                let _ = app.emit(
                    "downloads:event",
                    DownloadEvent {
                        kind: "error".to_string(),
                        tasks: None,
                        task_groups: None,
                        task: None,
                        error: Some(format!("收藏夹写入失败：{error}")),
                    },
                );
            }
            mark_completed(&app, &state.store, &task_id, retry_generation).await?;
        }
        return Ok(());
    }
    mark_failed(
        &app,
        &state.store,
        &task_id,
        retry_generation,
        &format!("all mirrors failed: {}", errors.join("; ")),
    )
    .await?;
    Ok(())
}

async fn scan_songs_directory(
    songs_dir: &Path,
) -> Result<HashMap<String, LocalBeatmapset>, String> {
    let mut local = HashMap::new();
    let mut entries = fs::read_dir(songs_dir).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let file_type = entry.file_type().await.map_err(|e| e.to_string())?;
        let folder_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_file() {
            if folder_path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("osz"))
            {
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

async fn scan_lazer_directory(
    lazer_dir: PathBuf,
) -> Result<HashMap<String, LocalBeatmapset>, String> {
    let files_dir = lazer_dir.join("files");
    if !files_dir.is_dir() {
        return Err("Selected folder does not look like an osu! lazer directory.".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let mut local = HashMap::new();
        let roots = lazer_scan_roots(&files_dir);
        let mut handles = Vec::new();

        for root in roots {
            handles.push(std::thread::spawn(move || scan_lazer_root(root)));
        }

        for handle in handles {
            if let Ok(scanned) = handle.join() {
                local.extend(scanned);
            }
        }

        Ok(local)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn lazer_scan_roots(files_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(entries) = std::fs::read_dir(files_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                roots.push(path);
            }
        }
    }
    if roots.is_empty() {
        roots.push(files_dir.to_path_buf());
    }
    roots
}

fn scan_lazer_root(root: PathBuf) -> HashMap<String, LocalBeatmapset> {
    let mut local = HashMap::new();
    let mut stack = vec![root];
    let mut buffer = vec![0_u8; LAZER_SCAN_READ_BYTES];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.len() == 0 || metadata.len() > LAZER_MAX_BEATMAP_BYTES {
                continue;
            }
            let mut file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            let read_len = match file.read(&mut buffer) {
                Ok(read_len) => read_len,
                Err(_) => continue,
            };
            let head = &buffer[..read_len];
            if !head.starts_with(b"osu file format")
                || !head
                    .windows(b"BeatmapSetID:".len())
                    .any(|w| w == b"BeatmapSetID:")
            {
                continue;
            }
            if let Some(id) = beatmapset_id_from_osu_bytes(head) {
                local
                    .entry(id.to_string())
                    .or_insert_with(|| local_entry(id, &path, "lazer-file"));
            }
        }
    }

    local
}

async fn move_completed_file(temp_path: &str, target_path: &str) -> Result<(), String> {
    match fs::rename(temp_path, target_path).await {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            fs::copy(temp_path, target_path)
                .await
                .map_err(|copy_error| {
                    format!("move failed: {rename_error}; copy failed: {copy_error}")
                })?;
            fs::remove_file(temp_path).await.map_err(|remove_error| {
                format!("download moved but cache cleanup failed: {remove_error}")
            })?;
            Ok(())
        }
    }
}

fn should_stage_playlist_group(task: &DownloadTask) -> bool {
    task.group_source.starts_with("歌单：")
        && task.group_destination.starts_with("写入收藏夹：")
        && task.download_mode != "osu"
}

async fn stage_completed_download(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    task: &mut DownloadTask,
) -> Result<(), String> {
    let staged_path = staged_download_path(app, task)?;
    if let Some(parent) = staged_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    move_completed_file(&task.temp_path, &staged_path.to_string_lossy()).await?;
    task.temp_path = staged_path.to_string_lossy().to_string();
    let tasks = {
        let mut store = store.lock().await;
        let Some(stored_task) = store.tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(());
        };
        if stored_task.retry_generation != retry_generation {
            return Ok(());
        }
        stored_task.temp_path = task.temp_path.clone();
        stored_task.status = "staged".to_string();
        stored_task.error = "已缓存，等待同任务全部下载完成".to_string();
        stored_task.updated_at = Utc::now().to_rfc3339();
        store.tasks.clone()
    };
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: Some(tasks),
            task_groups: None,
            task: None,
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

async fn try_finalize_staged_group(
    app: &tauri::AppHandle,
    store: &SharedStore,
    group_id: &str,
) -> Result<(), String> {
    finalize_staged_group(app, store, group_id, false).await
}

async fn finalize_staged_group(
    app: &tauri::AppHandle,
    store: &SharedStore,
    group_id: &str,
    allow_partial: bool,
) -> Result<(), String> {
    let (settings, all_group_tasks) = {
        let store = store.lock().await;
        let group_tasks = store
            .tasks
            .iter()
            .filter(|task| normalized_group_id(task) == group_id)
            .cloned()
            .collect::<Vec<_>>();
        (store.settings.clone(), group_tasks)
    };
    if all_group_tasks.is_empty() {
        return Ok(());
    }
    if !allow_partial && !all_group_tasks.iter().all(|task| task.status == "staged") {
        return Ok(());
    }
    let group_tasks = all_group_tasks
        .iter()
        .filter(|task| task.status == "staged")
        .cloned()
        .collect::<Vec<_>>();
    if group_tasks.is_empty() {
        return Err("这个任务还没有已缓存完成的歌曲，无法强制结束。".to_string());
    }
    if settings.stable_osu_dir.trim().is_empty() {
        return Err("请先选择 osu!stable 根目录。".to_string());
    }
    let stable_dir = PathBuf::from(settings.stable_osu_dir);
    let collection_name = non_empty_or_default(&settings.collection_name, "Seekman Downloads");
    let hash_tasks = group_tasks.clone();
    let hashes = tokio::task::spawn_blocking(move || {
        let mut hashes = Vec::new();
        for task in &hash_tasks {
            let allowed = task
                .collection_beatmap_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            let mut task_hashes = beatmap_md5s_from_osz(
                Path::new(&task.temp_path),
                if allowed.is_empty() {
                    None
                } else {
                    Some(&allowed)
                },
            )?;
            hashes.append(&mut task_hashes);
        }
        hashes.sort();
        hashes.dedup();
        Ok::<_, String>(hashes)
    })
    .await
    .map_err(|e| e.to_string())??;
    if hashes.is_empty() {
        return Err("歌单任务没有可写入收藏夹的子难度。".to_string());
    }
    for task in &group_tasks {
        if let Some(parent) = Path::new(&task.target_path).parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        fs::copy(&task.temp_path, &task.target_path)
            .await
            .map_err(|e| format!("转移到 Songs 失败：{e}"))?;
    }
    tokio::task::spawn_blocking(move || {
        add_hashes_to_collection(&stable_dir, &collection_name, hashes)
    })
    .await
    .map_err(|e| e.to_string())??;
    for task in &group_tasks {
        let _ = fs::remove_file(&task.temp_path).await;
    }
    if allow_partial {
        for task in &all_group_tasks {
            if task.status != "staged" {
                let _ = fs::remove_file(&task.temp_path).await;
            }
        }
    }
    let mut store = store.lock().await;
    for task in &group_tasks {
        store.local_beatmapsets.insert(
            task.beatmapset_id.to_string(),
            LocalBeatmapset {
                beatmapset_id: task.beatmapset_id,
                folder_path: task.target_path.clone(),
                detected_from: "download".to_string(),
                scanned_at: Utc::now().to_rfc3339(),
            },
        );
    }
    store
        .tasks
        .retain(|task| normalized_group_id(task) != group_id);
    store.task_groups.remove(group_id);
    save_store(app, &store).await?;
    emit_tasks(app, &store)
}

async fn add_download_to_collection_if_enabled(
    store: &SharedStore,
    task: &DownloadTask,
) -> Result<(), String> {
    if task.download_mode == "osu" {
        return Ok(());
    }
    let settings = store.lock().await.settings.clone();
    if !settings.collection_auto_add {
        return Ok(());
    }
    if settings.stable_osu_dir.trim().is_empty() {
        return Err("请先在实验性功能中选择 osu!stable 根目录。".to_string());
    }
    let collection_name = non_empty_or_default(&settings.collection_name, "Seekman Downloads");
    let stable_dir = PathBuf::from(settings.stable_osu_dir);
    let target_path = PathBuf::from(&task.target_path);
    let allowed_beatmap_ids = task
        .collection_beatmap_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    tokio::task::spawn_blocking(move || {
        let hashes = beatmap_md5s_from_osz(
            &target_path,
            if allowed_beatmap_ids.is_empty() {
                None
            } else {
                Some(&allowed_beatmap_ids)
            },
        )?;
        if hashes.is_empty() {
            return Err("下载文件中没有找到可写入收藏夹的 .osu 谱面文件。".to_string());
        }
        add_hashes_to_collection(&stable_dir, &collection_name, hashes)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn beatmap_md5s_from_osz(
    path: &Path,
    allowed_beatmap_ids: Option<&HashSet<u64>>,
) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开 osz 失败：{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 osz 失败：{e}"))?;
    let mut hashes = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 osz 条目失败：{e}"))?;
        if !entry
            .name()
            .rsplit('/')
            .next()
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".osu"))
        {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("读取 .osu 谱面失败：{e}"))?;
        if let Some(allowed) = allowed_beatmap_ids {
            let Some(beatmap_id) = beatmap_id_from_osu_bytes(&bytes) else {
                continue;
            };
            if !allowed.contains(&beatmap_id) {
                continue;
            }
        }
        let digest = Md5::digest(&bytes);
        hashes.push(format!("{digest:x}"));
    }
    hashes.sort();
    hashes.dedup();
    Ok(hashes)
}

fn beatmap_id_from_osu_bytes(bytes: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        let Some(value) = line.strip_prefix("BeatmapID:") else {
            continue;
        };
        if let Ok(id) = value.trim().parse::<u64>() {
            if id > 0 {
                return Some(id);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
struct StableCollection {
    version: i32,
    lists: Vec<StableCollectionList>,
}

#[derive(Debug, Clone)]
struct StableCollectionList {
    name: String,
    hashes: Vec<String>,
}

fn add_hashes_to_collection(
    stable_dir: &Path,
    collection_name: &str,
    hashes: Vec<String>,
) -> Result<(), String> {
    if !stable_dir.is_dir() {
        return Err("选择的 osu!stable 目录不存在。".to_string());
    }
    let db_path = stable_dir.join("collection.db");
    let mut collection = read_collection_db(&db_path)?;
    let list_index = collection
        .lists
        .iter()
        .position(|list| list.name == collection_name)
        .unwrap_or_else(|| {
            collection.lists.push(StableCollectionList {
                name: collection_name.to_string(),
                hashes: Vec::new(),
            });
            collection.lists.len() - 1
        });
    let list = &mut collection.lists[list_index];
    let mut existing = list.hashes.iter().cloned().collect::<HashSet<_>>();
    for hash in hashes {
        if existing.insert(hash.clone()) {
            list.hashes.push(hash);
        }
    }
    list.hashes.sort();
    backup_collection_db(&db_path)?;
    write_collection_db(&db_path, &collection)
}

fn read_collection_db(path: &Path) -> Result<StableCollection, String> {
    if !path.exists() {
        return Ok(StableCollection {
            version: 20260412,
            lists: Vec::new(),
        });
    }
    let bytes = std::fs::read(path).map_err(|e| format!("读取 collection.db 失败：{e}"))?;
    let mut reader = Cursor::new(bytes.as_slice());
    let version = read_i32(&mut reader)?;
    let list_count = read_i32(&mut reader)?.max(0) as usize;
    let mut lists = Vec::with_capacity(list_count);
    for _ in 0..list_count {
        let name = read_osu_string(&mut reader)?;
        let hash_count = read_i32(&mut reader)?.max(0) as usize;
        let mut hashes = Vec::with_capacity(hash_count);
        for _ in 0..hash_count {
            hashes.push(read_osu_string(&mut reader)?);
        }
        lists.push(StableCollectionList { name, hashes });
    }
    Ok(StableCollection { version, lists })
}

fn write_collection_db(path: &Path, collection: &StableCollection) -> Result<(), String> {
    let mut bytes = Vec::new();
    write_i32(&mut bytes, collection.version);
    write_i32(&mut bytes, collection.lists.len() as i32);
    for list in &collection.lists {
        write_osu_string(&mut bytes, &list.name);
        write_i32(&mut bytes, list.hashes.len() as i32);
        for hash in &list.hashes {
            write_osu_string(&mut bytes, hash);
        }
    }
    let temp_path = path.with_extension("db.seekman.tmp");
    std::fs::write(&temp_path, bytes).map_err(|e| format!("写入临时 collection.db 失败：{e}"))?;
    std::fs::rename(&temp_path, path).map_err(|e| format!("替换 collection.db 失败：{e}"))
}

fn backup_collection_db(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    let backup = path.with_file_name(format!("collection.db.seekman-backup-{stamp}"));
    std::fs::copy(path, backup).map_err(|e| format!("备份 collection.db 失败：{e}"))?;
    Ok(())
}

async fn resolve_stable_osu_dir(
    input: Option<String>,
    store: &SharedStore,
) -> Result<PathBuf, String> {
    let value = match input {
        Some(value) if !value.trim().is_empty() => value,
        _ => store.lock().await.settings.stable_osu_dir.clone(),
    };
    if value.trim().is_empty() {
        return Err("请先选择 osu!stable 根目录。".to_string());
    }
    let stable_dir = PathBuf::from(value);
    if !stable_dir.is_dir() {
        return Err("选择的 osu!stable 目录不存在。".to_string());
    }
    Ok(stable_dir)
}

fn stable_beatmaps_to_items(
    beatmaps: Vec<StableBeatmapInfo>,
    exists_local: bool,
    source_collection: &str,
) -> Vec<BeatmapsetItem> {
    let mut grouped: HashMap<u64, BeatmapsetItem> = HashMap::new();
    for beatmap in beatmaps {
        if beatmap.beatmapset_id == 0 || beatmap.beatmap_id == 0 {
            continue;
        }
        let set_id = beatmap.beatmapset_id;
        let beatmap_id = beatmap.beatmap_id;
        let mode = if beatmap.mode.trim().is_empty() {
            "osu".to_string()
        } else {
            beatmap.mode.clone()
        };
        let item = grouped.entry(set_id).or_insert_with(|| BeatmapsetItem {
            id: set_id,
            title: beatmap.title.clone(),
            artist: beatmap.artist.clone(),
            creator: beatmap.creator.clone(),
            ranked_date: String::new(),
            status: "playlist".to_string(),
            modes: Vec::new(),
            min_stars: None,
            max_stars: None,
            min_od: None,
            max_od: None,
            min_hp: None,
            max_hp: None,
            min_cs: None,
            max_cs: None,
            min_ar: None,
            max_ar: None,
            min_bpm: None,
            max_bpm: None,
            min_length: None,
            max_length: None,
            key_counts: Vec::new(),
            beatmap_ids: Vec::new(),
            collection_beatmap_ids: Vec::new(),
            source_collection: source_collection.to_string(),
            playcount: 0,
            favourite_count: 0,
            exists_local: Some(exists_local),
        });
        if !item.modes.contains(&mode) {
            item.modes.push(mode.clone());
        }
        let ar = Some(beatmap.ar as f64);
        let cs = Some(beatmap.cs as f64);
        let hp = Some(beatmap.hp as f64);
        let od = Some(beatmap.od as f64);
        merge_min(&mut item.min_ar, ar);
        merge_max(&mut item.max_ar, ar);
        merge_min(&mut item.min_cs, cs);
        merge_max(&mut item.max_cs, cs);
        merge_min(&mut item.min_hp, hp);
        merge_max(&mut item.max_hp, hp);
        merge_min(&mut item.min_od, od);
        merge_max(&mut item.max_od, od);
        merge_min(&mut item.min_bpm, beatmap.bpm);
        merge_max(&mut item.max_bpm, beatmap.bpm);
        merge_min_u64(
            &mut item.min_length,
            positive_i32_to_u64(beatmap.drain_time)
                .or_else(|| positive_i32_to_u64(beatmap.total_time)),
        );
        merge_max_u64(
            &mut item.max_length,
            positive_i32_to_u64(beatmap.total_time)
                .or_else(|| positive_i32_to_u64(beatmap.drain_time)),
        );
        if mode == "mania" {
            let keys = beatmap.cs.round() as u8;
            if keys > 0 && !item.key_counts.contains(&keys) {
                item.key_counts.push(keys);
                item.key_counts.sort_unstable();
            }
        }
        if !item.beatmap_ids.contains(&beatmap_id) {
            item.beatmap_ids.push(beatmap_id);
            item.beatmap_ids.sort_unstable();
        }
        if !item.collection_beatmap_ids.contains(&beatmap_id) {
            item.collection_beatmap_ids.push(beatmap_id);
            item.collection_beatmap_ids.sort_unstable();
        }
    }
    let mut items = grouped.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));
    items
}

fn positive_i32_to_u64(value: i32) -> Option<u64> {
    if value > 0 {
        Some(value as u64)
    } else {
        None
    }
}

fn export_collection_playlist_inner(
    stable_dir: &Path,
    collection_name: &str,
    selected_beatmap_ids: Option<Vec<u64>>,
) -> Result<String, String> {
    let collection = read_collection_db(&stable_dir.join("collection.db"))?;
    let Some(list) = collection
        .lists
        .iter()
        .find(|list| list.name == collection_name)
    else {
        return Err(format!("没有找到收藏夹：{collection_name}"));
    };
    let beatmaps = read_stable_osu_db(&stable_dir.join("osu!.db"))?;
    let by_md5 = beatmaps
        .into_iter()
        .map(|beatmap| (beatmap.md5.to_ascii_lowercase(), beatmap))
        .collect::<HashMap<_, _>>();
    let selected = selected_beatmap_ids
        .unwrap_or_default()
        .into_iter()
        .filter(|value| *value > 0)
        .collect::<HashSet<_>>();
    let dir = seekman_playlist_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建歌单导出目录失败：{e}"))?;
    let safe_name = non_empty_or_default(&sanitize_file_name(collection_name), "playlist");
    let path = dir.join(format!(
        "{}-{}.csv",
        safe_name,
        Utc::now().format("%Y%m%d%H%M%S")
    ));
    let mut csv = String::from(
        "seekman_export_version,exported_at,source_collection,beatmapset_id,beatmap_id,artist,artist_unicode,title,title_unicode,creator,version,mode,md5,folder_name,osu_file_name,audio_file_name,ranked_status,hitcircles,sliders,spinners,ar,cs,hp,od,slider_velocity,drain_time,total_time,preview_time,bpm,source,tags,last_modification_time\n",
    );
    let exported_at = Utc::now().to_rfc3339();
    for hash in &list.hashes {
        if let Some(beatmap) = by_md5.get(&hash.to_ascii_lowercase()) {
            if beatmap.beatmapset_id == 0 || beatmap.beatmap_id == 0 {
                continue;
            }
            if !selected.is_empty() && !selected.contains(&beatmap.beatmap_id) {
                continue;
            }
            csv.push_str(
                &[
                    "2".to_string(),
                    csv_cell(&exported_at),
                    csv_cell(collection_name),
                    beatmap.beatmapset_id.to_string(),
                    beatmap.beatmap_id.to_string(),
                    csv_cell(&beatmap.artist),
                    csv_cell(&beatmap.artist_unicode),
                    csv_cell(&beatmap.title),
                    csv_cell(&beatmap.title_unicode),
                    csv_cell(&beatmap.creator),
                    csv_cell(&beatmap.version),
                    csv_cell(&beatmap.mode),
                    csv_cell(&beatmap.md5),
                    csv_cell(&beatmap.folder_name),
                    csv_cell(&beatmap.osu_file_name),
                    csv_cell(&beatmap.audio_file_name),
                    beatmap.ranked_status.to_string(),
                    beatmap.hitcircles.to_string(),
                    beatmap.sliders.to_string(),
                    beatmap.spinners.to_string(),
                    format!("{:.2}", beatmap.ar),
                    format!("{:.2}", beatmap.cs),
                    format!("{:.2}", beatmap.hp),
                    format!("{:.2}", beatmap.od),
                    format!("{:.4}", beatmap.slider_velocity),
                    beatmap.drain_time.to_string(),
                    beatmap.total_time.to_string(),
                    beatmap.preview_time.to_string(),
                    beatmap
                        .bpm
                        .map(|value| format!("{value:.3}"))
                        .unwrap_or_default(),
                    csv_cell(&beatmap.source),
                    csv_cell(&beatmap.tags),
                    beatmap.last_modification_time.to_string(),
                ]
                .join(","),
            );
        } else {
            continue;
        }
        csv.push('\n');
    }
    std::fs::write(&path, csv).map_err(|e| format!("写入歌单 CSV 失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn import_seekman_playlist_inner(
    path: &Path,
    local_sets: &HashMap<String, LocalBeatmapset>,
) -> Result<Vec<BeatmapsetItem>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("读取歌单失败：{e}"))?;
    let mut rows = raw.lines();
    let Some(header_line) = rows.next() else {
        return Ok(Vec::new());
    };
    let headers = parse_csv_line(header_line);
    let index = |name: &str| headers.iter().position(|header| header == name);
    let version_idx = index("seekman_export_version")
        .ok_or_else(|| "歌单格式过旧：请导入新版 Seekman 导出的 CSV。".to_string())?;
    let set_idx =
        index("beatmapset_id").ok_or_else(|| "歌单缺少 beatmapset_id 列。".to_string())?;
    let beatmap_idx = index("beatmap_id").ok_or_else(|| "歌单缺少 beatmap_id 列。".to_string())?;
    let artist_idx = index("artist").ok_or_else(|| "歌单缺少 artist 列。".to_string())?;
    let title_idx = index("title").ok_or_else(|| "歌单缺少 title 列。".to_string())?;
    let creator_idx = index("creator").ok_or_else(|| "歌单缺少 creator 列。".to_string())?;
    let mode_idx = index("mode").ok_or_else(|| "歌单缺少 mode 列。".to_string())?;
    let source_idx =
        index("source_collection").ok_or_else(|| "歌单缺少 source_collection 列。".to_string())?;
    let ar_idx = index("ar").ok_or_else(|| "歌单缺少 ar 列。".to_string())?;
    let cs_idx = index("cs").ok_or_else(|| "歌单缺少 cs 列。".to_string())?;
    let hp_idx = index("hp").ok_or_else(|| "歌单缺少 hp 列。".to_string())?;
    let od_idx = index("od").ok_or_else(|| "歌单缺少 od 列。".to_string())?;
    let bpm_idx = index("bpm").ok_or_else(|| "歌单缺少 bpm 列。".to_string())?;
    let drain_time_idx =
        index("drain_time").ok_or_else(|| "歌单缺少 drain_time 列。".to_string())?;
    let total_time_idx =
        index("total_time").ok_or_else(|| "歌单缺少 total_time 列。".to_string())?;
    let mut grouped: HashMap<u64, BeatmapsetItem> = HashMap::new();
    for line in rows {
        if line.trim().is_empty() {
            continue;
        }
        let cells = parse_csv_line(line);
        if cells
            .get(version_idx)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            continue;
        }
        let Some(set_id) = cells
            .get(set_idx)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        if set_id == 0 {
            continue;
        }
        let Some(beatmap_id) = cells
            .get(beatmap_idx)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
        else {
            continue;
        };
        let artist = cells
            .get(artist_idx)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "Imported".to_string());
        let title = cells
            .get(title_idx)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| format!("Beatmapset #{set_id}"));
        let creator = cells
            .get(creator_idx)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| {
                cells
                    .get(source_idx)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| format!("歌单：{value}"))
                    .unwrap_or_else(|| "歌单导入".to_string())
            });
        let mode = cells
            .get(mode_idx)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "osu".to_string());
        let source_collection = cells
            .get(source_idx)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "导入歌单".to_string());
        let ar = parse_f64(cells.get(ar_idx).map(String::as_str));
        let cs = parse_f64(cells.get(cs_idx).map(String::as_str));
        let hp = parse_f64(cells.get(hp_idx).map(String::as_str));
        let od = parse_f64(cells.get(od_idx).map(String::as_str));
        let bpm = parse_f64(cells.get(bpm_idx).map(String::as_str));
        let drain_time = cells
            .get(drain_time_idx)
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .map(|value| value as u64);
        let total_time = cells
            .get(total_time_idx)
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .map(|value| value as u64);
        let item = grouped.entry(set_id).or_insert_with(|| BeatmapsetItem {
            id: set_id,
            title,
            artist,
            creator,
            ranked_date: String::new(),
            status: "playlist".to_string(),
            modes: Vec::new(),
            min_stars: None,
            max_stars: None,
            min_od: None,
            max_od: None,
            min_hp: None,
            max_hp: None,
            min_cs: None,
            max_cs: None,
            min_ar: None,
            max_ar: None,
            min_bpm: None,
            max_bpm: None,
            min_length: None,
            max_length: None,
            key_counts: Vec::new(),
            beatmap_ids: Vec::new(),
            collection_beatmap_ids: Vec::new(),
            source_collection,
            playcount: 0,
            favourite_count: 0,
            exists_local: Some(local_sets.contains_key(&set_id.to_string())),
        });
        if !item.modes.contains(&mode) {
            item.modes.push(mode.clone());
        }
        merge_min(&mut item.min_ar, ar);
        merge_max(&mut item.max_ar, ar);
        merge_min(&mut item.min_cs, cs);
        merge_max(&mut item.max_cs, cs);
        merge_min(&mut item.min_hp, hp);
        merge_max(&mut item.max_hp, hp);
        merge_min(&mut item.min_od, od);
        merge_max(&mut item.max_od, od);
        merge_min(&mut item.min_bpm, bpm);
        merge_max(&mut item.max_bpm, bpm);
        merge_min_u64(&mut item.min_length, drain_time.or(total_time));
        merge_max_u64(&mut item.max_length, total_time.or(drain_time));
        if mode == "mania" {
            if let Some(keys) = cs
                .map(|value| value.round() as u8)
                .filter(|value| *value > 0)
            {
                if !item.key_counts.contains(&keys) {
                    item.key_counts.push(keys);
                    item.key_counts.sort_unstable();
                }
            }
        }
        if !item.beatmap_ids.contains(&beatmap_id) {
            item.beatmap_ids.push(beatmap_id);
        }
        if !item.collection_beatmap_ids.contains(&beatmap_id) {
            item.collection_beatmap_ids.push(beatmap_id);
        }
    }
    let mut items = grouped.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));
    Ok(items)
}

fn apply_local_playlist_items_to_collection_inner(
    stable_dir: &Path,
    collection_name: &str,
    items: Vec<BeatmapsetItem>,
    commit: bool,
) -> Result<PlaylistLocalApplyResult, String> {
    let beatmaps = read_stable_osu_db(&stable_dir.join("osu!.db"))?;
    let mut by_set: HashMap<u64, Vec<StableBeatmapInfo>> = HashMap::new();
    for beatmap in beatmaps {
        if beatmap.beatmapset_id > 0 && beatmap.beatmap_id > 0 && !beatmap.md5.trim().is_empty() {
            by_set
                .entry(beatmap.beatmapset_id)
                .or_default()
                .push(beatmap);
        }
    }

    let mut hashes = Vec::new();
    let mut missing_items = Vec::new();
    let mut applied_sets = HashSet::new();
    let mut applied_count = 0usize;

    for mut item in items {
        let wanted_ids = if item.collection_beatmap_ids.is_empty() {
            item.beatmap_ids.clone()
        } else {
            item.collection_beatmap_ids.clone()
        };
        let wanted = wanted_ids
            .iter()
            .copied()
            .filter(|value| *value > 0)
            .collect::<HashSet<_>>();
        if wanted.is_empty() {
            missing_items.push(item);
            continue;
        }
        let local = by_set.get(&item.id).cloned().unwrap_or_default();
        let mut found_ids = HashSet::new();
        for beatmap in local {
            if wanted.contains(&beatmap.beatmap_id) {
                hashes.push(beatmap.md5);
                found_ids.insert(beatmap.beatmap_id);
                applied_count += 1;
                applied_sets.insert(item.id);
            }
        }
        let missing_ids = wanted
            .into_iter()
            .filter(|beatmap_id| !found_ids.contains(beatmap_id))
            .collect::<Vec<_>>();
        if !missing_ids.is_empty() {
            item.beatmap_ids = missing_ids.clone();
            item.collection_beatmap_ids = missing_ids;
            item.exists_local = Some(false);
            missing_items.push(item);
        }
    }

    hashes.sort();
    hashes.dedup();
    if commit && !hashes.is_empty() {
        add_hashes_to_collection(stable_dir, collection_name, hashes)?;
    }

    Ok(PlaylistLocalApplyResult {
        applied_count,
        applied_beatmapset_count: applied_sets.len(),
        missing_count: missing_items.len(),
        missing_items,
    })
}

fn read_stable_osu_db(path: &Path) -> Result<Vec<StableBeatmapInfo>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    let mut reader = Cursor::new(bytes.as_slice());
    let version = read_i32(&mut reader)?;
    let _folder_count = read_i32(&mut reader)?;
    let _account_unlocked = read_bool(&mut reader)?;
    let _unlock_date = read_i64(&mut reader)?;
    let _player_name = read_osu_string(&mut reader)?;
    let beatmap_count = read_i32(&mut reader)?.max(0) as usize;
    let mut beatmaps = Vec::with_capacity(beatmap_count);
    for _ in 0..beatmap_count {
        let artist = read_osu_string(&mut reader)?;
        let artist_unicode = read_osu_string(&mut reader)?;
        let title = read_osu_string(&mut reader)?;
        let title_unicode = read_osu_string(&mut reader)?;
        let creator = read_osu_string(&mut reader)?;
        let difficulty = read_osu_string(&mut reader)?;
        let audio_file_name = read_osu_string(&mut reader)?;
        let md5 = read_osu_string(&mut reader)?;
        let osu_file_name = read_osu_string(&mut reader)?;
        let ranked_status = read_u8(&mut reader)?;
        let hitcircles = read_i16(&mut reader)?;
        let sliders = read_i16(&mut reader)?;
        let spinners = read_i16(&mut reader)?;
        let last_modification_time = read_i64(&mut reader)?;
        let (ar, cs, hp, od) = if version < 20140609 {
            (
                f32::from(read_u8(&mut reader)?),
                f32::from(read_u8(&mut reader)?),
                f32::from(read_u8(&mut reader)?),
                f32::from(read_u8(&mut reader)?),
            )
        } else {
            (
                read_f32(&mut reader)?,
                read_f32(&mut reader)?,
                read_f32(&mut reader)?,
                read_f32(&mut reader)?,
            )
        };
        let slider_velocity = read_f64(&mut reader)?;
        if version >= 20140609 {
            for _ in 0..4 {
                skip_star_rating_pairs(&mut reader)?;
            }
        }
        let drain_time = read_i32(&mut reader)?;
        let total_time = read_i32(&mut reader)?;
        let preview_time = read_i32(&mut reader)?;
        let timing_points = read_i32(&mut reader)?.max(0) as usize;
        let mut bpm = None;
        for _ in 0..timing_points {
            let point_bpm = read_f64(&mut reader)?;
            let _offset = read_f64(&mut reader)?;
            let inherited = read_bool(&mut reader)?;
            if !inherited && point_bpm > 0.0 && bpm.is_none() {
                bpm = Some(point_bpm);
            }
        }
        let beatmap_id = read_i32(&mut reader)?.max(0) as u64;
        let beatmapset_id = read_i32(&mut reader)?.max(0) as u64;
        let _thread_id = read_i32(&mut reader)?;
        for _ in 0..4 {
            let _grade = read_u8(&mut reader)?;
        }
        let _local_offset = read_i16(&mut reader)?;
        let _stack_leniency = read_f32(&mut reader)?;
        let mode = stable_mode_name(read_u8(&mut reader)?).to_string();
        let source = read_osu_string(&mut reader)?;
        let tags = read_osu_string(&mut reader)?;
        let _online_offset = read_i16(&mut reader)?;
        let _title_font = read_osu_string(&mut reader)?;
        let _unplayed = read_bool(&mut reader)?;
        let _last_played = read_i64(&mut reader)?;
        let _is_osz2 = read_bool(&mut reader)?;
        let folder_name = read_osu_string(&mut reader)?;
        let _last_check = read_i64(&mut reader)?;
        let _ignore_sound = read_bool(&mut reader)?;
        let _ignore_skin = read_bool(&mut reader)?;
        let _disable_storyboard = read_bool(&mut reader)?;
        let _disable_video = read_bool(&mut reader)?;
        let _visual_override = read_bool(&mut reader)?;
        let _last_edit = read_i32(&mut reader)?;
        let _mania_scroll_speed = read_u8(&mut reader)?;
        beatmaps.push(StableBeatmapInfo {
            beatmapset_id,
            beatmap_id,
            artist,
            title,
            creator,
            version: difficulty,
            mode,
            md5,
            artist_unicode,
            title_unicode,
            audio_file_name,
            osu_file_name,
            ranked_status,
            hitcircles,
            sliders,
            spinners,
            last_modification_time,
            ar,
            cs,
            hp,
            od,
            slider_velocity,
            drain_time,
            total_time,
            preview_time,
            bpm,
            source,
            tags,
            folder_name,
        });
    }
    Ok(beatmaps)
}

fn skip_star_rating_pairs(reader: &mut Cursor<&[u8]>) -> Result<(), String> {
    let count = read_i32(reader)?.max(0) as usize;
    for _ in 0..count {
        let int_marker = read_u8(reader)?;
        if int_marker != 8 {
            return Err("osu!.db 星数缓存格式无法识别。".to_string());
        }
        let _mods = read_i32(reader)?;
        match read_u8(reader)? {
            12 => {
                let _value = read_f32(reader)?;
            }
            13 => {
                let _value = read_f64(reader)?;
            }
            _ => return Err("osu!.db 星数缓存数值格式无法识别。".to_string()),
        }
    }
    Ok(())
}

fn seekman_playlist_dir() -> Result<PathBuf, String> {
    let root = std::env::current_dir().map_err(|e| format!("读取当前目录失败：{e}"))?;
    Ok(root.join("seekman-playlists"))
}

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                let _ = chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                cells.push(cell);
                cell = String::new();
            }
            _ => cell.push(ch),
        }
    }
    cells.push(cell);
    cells
}

fn merge_min(target: &mut Option<f64>, value: Option<f64>) {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return;
    };
    *target = Some(target.map_or(value, |current| current.min(value)));
}

fn merge_max(target: &mut Option<f64>, value: Option<f64>) {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return;
    };
    *target = Some(target.map_or(value, |current| current.max(value)));
}

fn merge_min_u64(target: &mut Option<u64>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    *target = Some(target.map_or(value, |current| current.min(value)));
}

fn merge_max_u64(target: &mut Option<u64>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    *target = Some(target.map_or(value, |current| current.max(value)));
}

fn stable_mode_name(value: u8) -> &'static str {
    match value {
        1 => "taiko",
        2 => "fruits",
        3 => "mania",
        _ => "osu",
    }
}

fn read_u8(reader: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut bytes = [0_u8; 1];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    Ok(bytes[0])
}

fn read_bool(reader: &mut Cursor<&[u8]>) -> Result<bool, String> {
    Ok(read_u8(reader)? != 0)
}

fn read_i16(reader: &mut Cursor<&[u8]>) -> Result<i16, String> {
    let mut bytes = [0_u8; 2];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    Ok(i16::from_le_bytes(bytes))
}

fn read_i64(reader: &mut Cursor<&[u8]>) -> Result<i64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_f32(reader: &mut Cursor<&[u8]>) -> Result<f32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    Ok(f32::from_le_bytes(bytes))
}

fn read_f64(reader: &mut Cursor<&[u8]>) -> Result<f64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 osu!.db 失败：{e}"))?;
    Ok(f64::from_le_bytes(bytes))
}

fn read_i32(reader: &mut Cursor<&[u8]>) -> Result<i32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取 collection.db 失败：{e}"))?;
    Ok(i32::from_le_bytes(bytes))
}

fn write_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_osu_string(reader: &mut Cursor<&[u8]>) -> Result<String, String> {
    let mut marker = [0_u8; 1];
    reader
        .read_exact(&mut marker)
        .map_err(|e| format!("读取字符串失败：{e}"))?;
    if marker[0] == 0 {
        return Ok(String::new());
    }
    if marker[0] != 0x0b {
        return Err("collection.db 字符串格式无法识别。".to_string());
    }
    let len = read_uleb128(reader)?;
    let mut bytes = vec![0_u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("读取字符串内容失败：{e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("字符串不是 UTF-8：{e}"))
}

fn write_osu_string(bytes: &mut Vec<u8>, value: &str) {
    if value.is_empty() {
        bytes.push(0);
        return;
    }
    bytes.push(0x0b);
    write_uleb128(bytes, value.as_bytes().len());
    bytes.extend_from_slice(value.as_bytes());
}

fn read_uleb128(reader: &mut Cursor<&[u8]>) -> Result<usize, String> {
    let mut result = 0_usize;
    let mut shift = 0;
    loop {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|e| format!("读取字符串长度失败：{e}"))?;
        result |= ((byte[0] & 0x7f) as usize) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift > 28 {
            return Err("collection.db 字符串长度过大。".to_string());
        }
    }
}

fn write_uleb128(bytes: &mut Vec<u8>, mut value: usize) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
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

fn beatmapset_id_from_osu_bytes(bytes: &[u8]) -> Option<u64> {
    for line in bytes.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(value) = line.strip_prefix(b"BeatmapSetID:") {
            let digits = value
                .iter()
                .copied()
                .skip_while(|byte| byte.is_ascii_whitespace())
                .take_while(|byte| byte.is_ascii_digit())
                .collect::<Vec<_>>();
            let value = std::str::from_utf8(&digits).ok()?;
            return value.parse::<u64>().ok().filter(|id| *id > 0);
        }
    }
    None
}

async fn search_osu(
    client: &Client,
    token: &str,
    filters: &Filters,
) -> Result<Vec<BeatmapsetItem>, String> {
    let max_pages = filters
        .max_pages
        .as_deref()
        .unwrap_or("10")
        .parse::<usize>()
        .unwrap_or(10)
        .clamp(1, 50);
    let mut cursor = String::new();
    let mut results = Vec::new();
    for _ in 0..max_pages {
        let status = normalize_search_status(filters.status.as_deref());
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
            return Err(format!(
                "Beatmapset search failed: HTTP {}",
                response.status()
            ));
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
        cursor = data
            .get("cursor_string")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if cursor.is_empty() {
            break;
        }
    }
    let mut seen = HashSet::new();
    results.retain(|item| seen.insert(item.id));
    sort_results(&mut results, filters);
    Ok(results)
}

async fn search_alpha_osu(
    client: &Client,
    request: &AlphaRecommendRequest,
) -> Result<Vec<BeatmapsetItem>, String> {
    let username = request.username.trim();
    if username.is_empty() {
        return Err("Please input an AlphaOsu username.".to_string());
    }
    let login_response = client
        .post("https://alphaosu.keytoix.vip/api/v1/login")
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !login_response.status().is_success() {
        return Err(format!(
            "AlphaOsu login failed: HTTP {}",
            login_response.status()
        ));
    }
    let login: Value = login_response.json().await.map_err(|e| e.to_string())?;
    let login_data = alpha_data(&login)?;
    let uid = login_data
        .get("uid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "AlphaOsu login did not return uid.".to_string())?;
    let mode = alpha_mode_value(request.mode.as_deref())
        .or_else(|| login_data.get("gameMode").and_then(|v| v.as_u64()))
        .unwrap_or(3);
    let key_count = parse_u8(request.key_count.as_deref())
        .map(u64::from)
        .or_else(|| login_data.get("keyCount").and_then(|v| v.as_u64()))
        .unwrap_or(4);
    let mod_value = login_data
        .get("mod")
        .and_then(|v| v.as_array())
        .and_then(|values| values.first())
        .and_then(|v| v.as_str())
        .unwrap_or("NM");
    let limit = parse_u64(request.limit.as_deref())
        .unwrap_or(100)
        .clamp(1, 500) as usize;
    let page_size = limit.min(100);
    let mut current = 1_usize;
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    while results.len() < limit {
        let params = vec![
            ("current", current.to_string()),
            ("pageSize", page_size.to_string()),
            ("uid", uid.to_string()),
            ("gameMode", mode.to_string()),
            ("keyCount", key_count.to_string()),
            ("mod", mod_value.to_string()),
            ("passPercent", "0.2,1".to_string()),
            ("newRecordPercent", "0.2,1".to_string()),
            ("difficulty", "0,15".to_string()),
            ("hidePlayed", "0".to_string()),
            ("rule", "4".to_string()),
        ];
        let response = client
            .get("https://alphaosu.keytoix.vip/api/v1/self/maps/recommend")
            .query(&params)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "AlphaOsu recommendations failed: HTTP {}",
                response.status()
            ));
        }
        let data: Value = response.json().await.map_err(|e| e.to_string())?;
        let page_data = alpha_data(&data)?;
        let list = page_data
            .get("list")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "AlphaOsu response did not return a list.".to_string())?;
        if list.is_empty() {
            break;
        }
        for map in list {
            if results.len() >= limit {
                break;
            }
            if let Some(item) = alpha_map_to_item(map) {
                if seen.insert(item.id) {
                    results.push(item);
                }
            }
        }
        let next = page_data.get("next").and_then(|v| v.as_i64()).unwrap_or(-1);
        if next <= 0 {
            break;
        }
        current = next as usize;
    }

    Ok(results)
}

async fn mark_existing_items(items: &mut [BeatmapsetItem], state: &State<'_, RuntimeState>) {
    let local_ids = local_ids_for_selected_source(state).await;
    for item in items {
        item.exists_local = Some(local_ids.contains(&item.id.to_string()));
    }
}

async fn local_ids_for_selected_source(state: &State<'_, RuntimeState>) -> HashSet<String> {
    let store = state.store.lock().await;
    let local_source = store.settings.local_source.as_str();
    store
        .local_beatmapsets
        .iter()
        .filter(|(_, entry)| local_source_matches(local_source, &entry.detected_from))
        .map(|(id, _)| id.clone())
        .collect()
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
        return Err(
            "Please fill osu! OAuth Client ID and Client Secret, or paste a Bearer Token."
                .to_string(),
        );
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
    let token = data
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let expires_in = data
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    *state.token_cache.lock().await = Some(TokenCache {
        token: token.clone(),
        expires_at_ms: Utc::now().timestamp_millis() + expires_in * 1000,
    });
    Ok(token)
}

fn map_beatmapset(set: &Value, filters: &Filters) -> BeatmapsetItem {
    let beatmaps = set
        .get("beatmaps")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
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
        if let Some(value) = beatmap
            .get("bpm")
            .and_then(|v| v.as_f64())
            .or_else(|| set.get("bpm").and_then(|v| v.as_f64()))
        {
            bpms.push(value);
        }
        if let Some(value) = beatmap
            .get("total_length")
            .or_else(|| beatmap.get("hit_length"))
            .and_then(|v| v.as_u64())
        {
            lengths.push(value);
        }
        if let Some(value) = beatmap.get("mode").and_then(|v| v.as_str()) {
            modes.insert(value.to_string());
        }
        if beatmap.get("mode").and_then(|v| v.as_str()) == Some("mania") {
            if let Some(keys) = beatmap
                .get("cs")
                .and_then(|v| v.as_f64())
                .map(|v| v.round() as u8)
            {
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
        collection_beatmap_ids: Vec::new(),
        source_collection: String::new(),
        playcount: set
            .get("play_count")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        favourite_count: set
            .get("favourite_count")
            .and_then(|v| v.as_u64())
            .unwrap_or_default(),
        exists_local: Some(false),
    }
}

fn alpha_data(value: &Value) -> Result<&Value, String> {
    if value.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("AlphaOsu request failed.")
            .to_string());
    }
    value
        .get("data")
        .ok_or_else(|| "AlphaOsu response did not contain data.".to_string())
}

fn alpha_map_to_item(map: &Value) -> Option<BeatmapsetItem> {
    let beatmap_id = alpha_beatmap_id(map)?;
    let beatmapset_id = alpha_beatmapset_id(map).unwrap_or(beatmap_id);
    let map_name = map.get("mapName").and_then(|v| v.as_str()).unwrap_or("");
    let (artist, title) = split_alpha_map_name(map_name);
    let stars = map.get("difficulty").and_then(|v| v.as_f64());
    let bpm = map.get("bpm").and_then(|v| v.as_f64());
    let length = map
        .get("length")
        .and_then(|v| v.as_f64())
        .map(|v| v.round().max(0.0) as u64);
    let key_count = map
        .get("keyCount")
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .filter(|v| *v > 0);
    let predict_pp = map.get("predictPP").and_then(|v| v.as_f64());
    let increment = map.get("ppIncrementExpect").and_then(|v| v.as_f64());
    let pass_percent = map.get("passPercent").and_then(|v| v.as_f64());
    let creator = format!(
        "AlphaOsu! · 预测PP {} · PP潜力 {} · 通过率 {}",
        format_optional_number(predict_pp, 1),
        format_optional_number(increment, 1),
        format_optional_percent(pass_percent)
    );

    Some(BeatmapsetItem {
        id: beatmapset_id,
        title,
        artist,
        creator,
        ranked_date: String::new(),
        status: "alphaosu".to_string(),
        modes: vec![if key_count.is_some() {
            "mania".to_string()
        } else {
            "osu".to_string()
        }],
        min_stars: stars,
        max_stars: stars,
        min_od: None,
        max_od: None,
        min_hp: None,
        max_hp: None,
        min_cs: key_count.map(f64::from),
        max_cs: key_count.map(f64::from),
        min_ar: None,
        max_ar: None,
        min_bpm: bpm,
        max_bpm: bpm,
        min_length: length,
        max_length: length,
        key_counts: key_count.into_iter().collect(),
        beatmap_ids: vec![beatmap_id],
        collection_beatmap_ids: Vec::new(),
        source_collection: String::new(),
        playcount: 0,
        favourite_count: 0,
        exists_local: Some(false),
    })
}

fn alpha_beatmap_id(map: &Value) -> Option<u64> {
    map.get("id")
        .and_then(|v| v.as_str())
        .and_then(|value| value.split('/').next())
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            map.get("mapLink")
                .and_then(|v| v.as_str())
                .and_then(last_number_in_url)
        })
}

fn alpha_beatmapset_id(map: &Value) -> Option<u64> {
    map.get("mapCoverUrl")
        .and_then(|v| v.as_str())
        .and_then(|value| value.split("/beatmaps/").nth(1))
        .and_then(|value| value.split('/').next())
        .and_then(|value| value.parse::<u64>().ok())
}

fn split_alpha_map_name(value: &str) -> (String, String) {
    if let Some((artist, title)) = value.split_once(" - ") {
        (artist.trim().to_string(), title.trim().to_string())
    } else {
        ("AlphaOsu!".to_string(), value.trim().to_string())
    }
}

fn alpha_mode_value(value: Option<&str>) -> Option<u64> {
    match value {
        Some("osu") | Some("std") => Some(0),
        Some("taiko") => Some(1),
        Some("fruits") | Some("ctb") => Some(2),
        Some("mania") => Some(3),
        _ => None,
    }
}

fn last_number_in_url(value: &str) -> Option<u64> {
    value
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
}

fn format_optional_number(value: Option<f64>, decimals: usize) -> String {
    value
        .map(|v| format!("{v:.decimals$}"))
        .unwrap_or_else(|| "?".to_string())
}

fn format_optional_percent(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.1}%", v * 100.0))
        .unwrap_or_else(|| "?".to_string())
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
        if !item.ranked_date.is_empty()
            && item.ranked_date[..10.min(item.ranked_date.len())] < *from
        {
            return false;
        }
    }
    if let Some(to) = filters.date_to.as_deref().filter(|v| !v.is_empty()) {
        if !item.ranked_date.is_empty() && item.ranked_date[..10.min(item.ranked_date.len())] > *to
        {
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
    if let Some(q) = filters
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        parts.push(q.to_string());
    }
    if let Some(from) = filters
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("ranked>={from}"));
    }
    if let Some(to) = filters
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
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
        (Some("stars") | Some("difficulty"), Some("asc")) => "difficulty_asc",
        (Some("stars") | Some("difficulty"), _) => "difficulty_desc",
        (Some("relevance"), _) => "relevance_desc",
        (Some("time") | None, Some("asc")) => "ranked_asc",
        (Some("time") | None, _) => "ranked_desc",
        _ => "ranked_desc",
    }
}

fn sort_results(items: &mut [BeatmapsetItem], filters: &Filters) {
    let ascending = filters.sort_dir.as_deref() == Some("asc");
    match filters.sort_by.as_deref().unwrap_or("time") {
        "stars" | "difficulty" => sort_by_optional_f64(items, ascending, |item| item.max_stars),
        "length" => sort_by_optional_u64(items, ascending, |item| item.max_length),
        "bpm" => sort_by_optional_f64(items, ascending, |item| item.max_bpm),
        "relevance" => {}
        _ => {
            items.sort_by(|a, b| a.ranked_date.cmp(&b.ranked_date));
            if !ascending {
                items.reverse();
            }
        }
    }
}

fn sort_by_optional_u64(
    items: &mut [BeatmapsetItem],
    ascending: bool,
    get: fn(&BeatmapsetItem) -> Option<u64>,
) {
    items.sort_by(|a, b| get(a).unwrap_or_default().cmp(&get(b).unwrap_or_default()));
    if !ascending {
        items.reverse();
    }
}

fn sort_by_optional_f64(
    items: &mut [BeatmapsetItem],
    ascending: bool,
    get: fn(&BeatmapsetItem) -> Option<f64>,
) {
    items.sort_by(|a, b| {
        get(a)
            .unwrap_or_default()
            .total_cmp(&get(b).unwrap_or_default())
    });
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
            let beatmap_keys = beatmap
                .get("cs")
                .and_then(|v| v.as_f64())
                .map(|v| v.round() as u8);
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
    let mut store = match fs::read_to_string(path).await {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppStore::default(),
    };
    ensure_task_group_progress(&mut store);
    store
}

async fn save_store(app: &tauri::AppHandle, store: &AppStore) -> Result<(), String> {
    let path = store_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
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
    emit_tasks(app, &store)
}

fn emit_tasks(app: &tauri::AppHandle, store: &AppStore) -> Result<(), String> {
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "tasks".to_string(),
            tasks: Some(store.tasks.clone()),
            task_groups: Some(store.task_groups.clone()),
            task: None,
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

fn prune_empty_task_groups(store: &mut AppStore) {
    let active_groups = store
        .tasks
        .iter()
        .map(normalized_group_id)
        .collect::<HashSet<_>>();
    store.task_groups.retain(|group_id, group| {
        active_groups.contains(group_id) || group.completed_tasks < group.total_tasks
    });
}

fn ensure_task_group_progress(store: &mut AppStore) {
    let mut grouped: HashMap<String, Vec<DownloadTask>> = HashMap::new();
    for task in &store.tasks {
        grouped
            .entry(normalized_group_id(task))
            .or_default()
            .push(task.clone());
    }
    for (group_id, tasks) in grouped {
        if store.task_groups.contains_key(&group_id) {
            continue;
        }
        let Some(first) = tasks.first() else {
            continue;
        };
        let total_tasks = parse_task_total_from_group_name(&first.group_name)
            .unwrap_or(tasks.len())
            .max(tasks.len());
        let active_finished = tasks
            .iter()
            .filter(|task| is_finished_status(&task.status))
            .count();
        let completed_tasks = total_tasks
            .saturating_sub(tasks.len())
            .saturating_add(active_finished)
            .min(total_tasks);
        let completed_bytes = tasks
            .iter()
            .filter(|task| is_finished_status(&task.status))
            .map(|task| task.downloaded_bytes)
            .sum();
        store.task_groups.insert(
            group_id.clone(),
            DownloadGroupProgress {
                id: group_id,
                name: first.group_name.clone(),
                source: first.group_source.clone(),
                destination: first.group_destination.clone(),
                total_tasks,
                completed_tasks,
                completed_bytes,
                created_at: first.created_at.clone(),
                updated_at: Utc::now().to_rfc3339(),
            },
        );
    }
}

fn parse_task_total_from_group_name(value: &str) -> Option<usize> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<usize>().ok())
        .last()
        .filter(|value| *value > 0)
}

fn is_finished_status(status: &str) -> bool {
    status == "completed" || status == "staged"
}

fn mark_group_task_completed(store: &mut AppStore, task: &DownloadTask) {
    let group_id = normalized_group_id(task);
    let group = store
        .task_groups
        .entry(group_id.clone())
        .or_insert_with(|| DownloadGroupProgress {
            id: group_id,
            name: task.group_name.clone(),
            source: task.group_source.clone(),
            destination: task.group_destination.clone(),
            total_tasks: 1,
            completed_tasks: 0,
            completed_bytes: 0,
            created_at: task.created_at.clone(),
            updated_at: task.updated_at.clone(),
        });
    if group.total_tasks == 0 {
        group.total_tasks = 1;
    }
    group.completed_tasks = group
        .completed_tasks
        .saturating_add(1)
        .min(group.total_tasks);
    group.completed_bytes = group.completed_bytes.saturating_add(task.downloaded_bytes);
    group.updated_at = Utc::now().to_rfc3339();
}

async fn is_attempt_current(store: &SharedStore, id: &str, retry_generation: u64) -> bool {
    let store = store.lock().await;
    store
        .tasks
        .iter()
        .find(|task| task.id == id)
        .is_some_and(|task| task.retry_generation == retry_generation && task.status != "cancelled")
}

async fn update_progress(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    downloaded: u64,
    total: Option<u64>,
) -> Result<(), String> {
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
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: Some(tasks),
            task_groups: None,
            task: Some(task),
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

async fn update_task_attempt(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    url: &str,
    error: &str,
) -> Result<(), String> {
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
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: None,
            task_groups: None,
            task: Some(task),
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

async fn reset_stalled_attempt(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    temp_path: &str,
    error: &str,
) -> Result<(), String> {
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
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: Some(tasks),
            task_groups: None,
            task: Some(task),
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

async fn mark_failed_latest(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    error: &str,
) -> Result<(), String> {
    let (task, tasks) = {
        let mut store = store.lock().await;
        let Some(task) = store.tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(());
        };
        if task.status != "downloading" {
            return Ok(());
        }
        task.status = "failed".to_string();
        task.error = error.to_string();
        task.updated_at = Utc::now().to_rfc3339();
        (task.clone(), store.tasks.clone())
    };
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: Some(tasks),
            task_groups: None,
            task: Some(task),
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

async fn mark_paused(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "paused", "").await
}

async fn mark_failed(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    error: &str,
) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "failed", error).await
}

async fn mark_completed(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
) -> Result<(), String> {
    set_status(app, store, id, retry_generation, "completed", "").await
}

async fn set_status(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    status: &str,
    error: &str,
) -> Result<(), String> {
    let mut data = store.lock().await;
    let completed_info = if let Some(index) = data.tasks.iter().position(|task| task.id == id) {
        if data.tasks[index].retry_generation != retry_generation {
            return Ok(());
        }
        if status == "completed" {
            let task = data.tasks.remove(index);
            mark_group_task_completed(&mut data, &task);
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
        data.local_beatmapsets.insert(
            beatmapset_id.to_string(),
            LocalBeatmapset {
                beatmapset_id,
                folder_path: target_path,
                detected_from: "download".to_string(),
                scanned_at: Utc::now().to_rfc3339(),
            },
        );
    }
    save_store(app, &data).await?;
    emit_tasks(app, &data)
}

fn merge_settings(settings: &mut Settings, value: Value) {
    if let Some(v) = value.get("songsDir").and_then(|v| v.as_str()) {
        settings.songs_dir = v.to_string();
    }
    if let Some(v) = value.get("lazerDir").and_then(|v| v.as_str()) {
        settings.lazer_dir = v.to_string();
    }
    if let Some(v) = value.get("stableOsuDir").and_then(|v| v.as_str()) {
        settings.stable_osu_dir = v.to_string();
    }
    if let Some(v) = value.get("collectionAutoAdd").and_then(|v| v.as_bool()) {
        settings.collection_auto_add = v;
    }
    if let Some(v) = value.get("collectionName").and_then(|v| v.as_str()) {
        settings.collection_name = non_empty_or_default(v, "Seekman Downloads");
    }
    if let Some(v) = value.get("localSource").and_then(|v| v.as_str()) {
        settings.local_source = normalize_local_source(v).to_string();
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
        settings.concurrent_downloads = (v as usize).clamp(1, 64);
    }
    if let Some(v) = value.get("includeVideo").and_then(|v| v.as_bool()) {
        settings.include_video = v;
        if settings.download_mode != "osu" {
            settings.download_mode = if v {
                "video".to_string()
            } else {
                "noVideo".to_string()
            };
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
    if let Some(v) = value.get("theme").and_then(|v| v.as_str()) {
        settings.theme = normalize_theme(v).to_string();
    }
    if let Some(v) = value.get("dismissedUpdateVersion").and_then(|v| v.as_str()) {
        settings.dismissed_update_version = normalize_version_tag(v);
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

fn non_empty_or_default(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
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

fn replace_local_source<F>(
    current: &mut HashMap<String, LocalBeatmapset>,
    scanned: HashMap<String, LocalBeatmapset>,
    should_replace: F,
) where
    F: Fn(&str) -> bool,
{
    current.retain(|_, entry| !should_replace(&entry.detected_from));
    current.extend(scanned);
}

fn normalize_local_source(value: &str) -> &str {
    match value {
        "lazer" => "lazer",
        _ => "stable",
    }
}

fn local_source_matches(local_source: &str, detected_from: &str) -> bool {
    match normalize_local_source(local_source) {
        "lazer" => detected_from.starts_with("lazer"),
        _ => !detected_from.starts_with("lazer"),
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

async fn prepare_runtime_temp_path(
    app: &tauri::AppHandle,
    store: &SharedStore,
    id: &str,
    retry_generation: u64,
    task: &mut DownloadTask,
) -> Result<(), String> {
    let runtime_path = runtime_temp_path(app, task)?;
    let runtime_value = runtime_path.to_string_lossy().to_string();
    if task.temp_path == runtime_value {
        return Ok(());
    }
    let old_path = task.temp_path.clone();
    task.temp_path = runtime_value.clone();
    let updated = {
        let mut store = store.lock().await;
        let Some(stored_task) = store
            .tasks
            .iter_mut()
            .find(|stored_task| stored_task.id == id)
        else {
            return Ok(());
        };
        if stored_task.retry_generation != retry_generation {
            return Ok(());
        }
        stored_task.temp_path = runtime_value;
        stored_task.clone()
    };
    let _ = fs::remove_file(old_path).await;
    app.emit(
        "downloads:event",
        DownloadEvent {
            kind: "progress".to_string(),
            tasks: None,
            task_groups: None,
            task: Some(updated),
            error: None,
        },
    )
    .map_err(|e| e.to_string())
}

fn runtime_temp_path(app: &tauri::AppHandle, task: &DownloadTask) -> Result<PathBuf, String> {
    Ok(runtime_download_cache_dir(app)?.join(temp_file_name(task)))
}

fn staged_download_path(app: &tauri::AppHandle, task: &DownloadTask) -> Result<PathBuf, String> {
    let name = Path::new(&task.target_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("seekman-download.osz");
    Ok(runtime_download_cache_dir(app)?
        .join("staged")
        .join(sanitize_file_name(&normalized_group_id(task)))
        .join(name))
}

fn runtime_download_cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        return app
            .path()
            .app_cache_dir()
            .or_else(|_| app.path().app_data_dir())
            .map(|path| path.join("download-cache"))
            .map_err(|e| e.to_string());
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(download_cache_dir())
    }
}

fn temp_file_name(task: &DownloadTask) -> String {
    Path::new(&task.temp_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            fresh_temp_path(task)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("seekman-download.part")
                .to_string()
        })
}

fn normalize_search_status(value: Option<&str>) -> &'static str {
    match value {
        Some("loved") => "loved",
        Some("graveyard") | Some("grave") => "graveyard",
        _ => "ranked",
    }
}

fn recreate_retry_task(task: &mut DownloadTask, settings: &Settings) {
    task.id = fresh_task_id(task);
    task.status = "queued".to_string();
    task.error.clear();
    task.total_bytes = None;
    task.downloaded_bytes = 0;
    task.retry_generation = 0;
    task.url = first_download_url(task, settings);
    task.temp_path = fresh_temp_path(task).to_string_lossy().to_string();
    task.updated_at = Utc::now().to_rfc3339();
}

fn fresh_task_id(task: &DownloadTask) -> String {
    let id_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    if task.download_mode == "osu" {
        let beatmap_id = task.beatmap_id.unwrap_or(task.beatmapset_id);
        format!(
            "osu-{}-{}-{}",
            beatmap_id,
            Utc::now().timestamp_millis(),
            id_suffix
        )
    } else {
        format!(
            "{}-{}-{}",
            task.beatmapset_id,
            Utc::now().timestamp_millis(),
            id_suffix
        )
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

fn normalized_group_id(task: &DownloadTask) -> String {
    if task.group_id.trim().is_empty() {
        format!("legacy-{}", task.created_at)
    } else {
        task.group_id.clone()
    }
}

fn group_source_from_items(items: &[BeatmapsetItem]) -> String {
    let mut sources = items
        .iter()
        .filter_map(|item| {
            let value = item.source_collection.trim();
            if value.is_empty() {
                None
            } else {
                Some(format!("歌单：{value}"))
            }
        })
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    if sources.len() == 1 {
        sources.remove(0)
    } else if sources.len() > 1 {
        "多个歌单导入".to_string()
    } else {
        "搜索结果".to_string()
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

fn mirror_candidates_for_settings(
    id: u64,
    include_video: bool,
    settings: &Settings,
) -> Vec<MirrorCandidate> {
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
            if include_video {
                base
            } else {
                format!("{base}?noVideo=1")
            }
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
            if include_video {
                base
            } else {
                format!("{base}?noVideo=1")
            }
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
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control()
            {
                '_'
            } else {
                ch
            }
        })
        .take(180)
        .collect()
}

fn parse_f64(value: Option<&str>) -> Option<f64> {
    value.and_then(|v| {
        if v.trim().is_empty() {
            None
        } else {
            v.trim().parse().ok()
        }
    })
}

fn parse_u64(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| {
        if v.trim().is_empty() {
            None
        } else {
            v.trim().parse().ok()
        }
    })
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

fn normalize_theme(value: &str) -> &'static str {
    match value.trim() {
        "lime" | "BFFF00+222222" => "lime",
        "cyan" | "2C2C34+00D4FF" => "cyan",
        "sky" | "89C2FF+E6E7FF" => "sky",
        _ => "cyan",
    }
}

#[cfg(target_os = "android")]
async fn ensure_mobile_songs_dir(
    app: &tauri::AppHandle,
    store: &mut AppStore,
) -> Result<(), String> {
    if !should_migrate_android_songs_dir(&store.settings.songs_dir) {
        return Ok(());
    }
    let dir_path = ensure_android_songs_dir(app).await?;
    store.settings.songs_dir = dir_path.to_string_lossy().to_string();
    Ok(())
}

#[cfg(target_os = "android")]
async fn ensure_android_songs_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let preferred = android_default_songs_dir(app)?;
    if fs::create_dir_all(&preferred).await.is_ok() {
        return Ok(preferred);
    }
    let fallback = android_private_songs_dir(app)?;
    fs::create_dir_all(&fallback)
        .await
        .map_err(|e| e.to_string())?;
    Ok(fallback)
}

#[cfg(target_os = "android")]
fn android_default_songs_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let external = PathBuf::from("/storage/emulated/0")
        .join("Android")
        .join("data")
        .join("com.linnzero00.osubeatmapseekman")
        .join("files")
        .join("Songs");
    if external.is_absolute() {
        return Ok(external);
    }
    if let Ok(downloads) = app.path().download_dir() {
        return Ok(downloads.join("Osu Beatmap Seekman").join("Songs"));
    }
    android_private_songs_dir(app)
}

#[cfg(target_os = "android")]
fn should_migrate_android_songs_dir(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    normalized.is_empty()
        || normalized.starts_with("/data/")
        || !normalized.contains("/Android/data/com.linnzero00.osubeatmapseekman/files/")
}

#[cfg(target_os = "android")]
fn android_private_songs_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Songs"))
}

#[cfg(not(target_os = "android"))]
async fn ensure_mobile_songs_dir(
    _app: &tauri::AppHandle,
    _store: &mut AppStore,
) -> Result<(), String> {
    Ok(())
}

fn build_http_client() -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static(APP_REFERER),
    );
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static(APP_USER_AGENT),
    );
    Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .connect_timeout(Duration::from_secs(12))
        .read_timeout(Duration::from_secs(DOWNLOAD_STALL_TIMEOUT_SECS))
        .timeout(Duration::from_secs(180))
        .no_proxy()
        .build()
        .expect("failed to create HTTP client")
}

async fn fetch_latest_release(client: &Client) -> Result<GithubRelease, String> {
    let response = client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub update check failed: {}", response.status()));
    }
    response
        .json::<GithubRelease>()
        .await
        .map_err(|e| e.to_string())
}

fn release_to_update_info(
    release: &GithubRelease,
    dismissed_version: &str,
) -> Result<Option<UpdateInfo>, String> {
    if release.draft || release.prerelease || !is_release_newer(&release.tag_name) {
        return Ok(None);
    }
    let version = normalize_version_tag(&release.tag_name);
    if normalize_version_tag(dismissed_version) == version {
        return Ok(None);
    }
    Ok(Some(UpdateInfo {
        version: version.clone(),
        name: release
            .name
            .clone()
            .unwrap_or_else(|| format!("Osu! Beatmap Seekman v{version}")),
        body: release.body.clone().unwrap_or_default(),
        html_url: release.html_url.clone(),
        published_at: release.published_at.clone().unwrap_or_default(),
        can_install_now: cfg!(target_os = "windows")
            && find_windows_installer_asset(release).is_some(),
    }))
}

fn is_release_newer(tag: &str) -> bool {
    compare_versions(&normalize_version_tag(tag), env!("CARGO_PKG_VERSION")).is_gt()
}

fn normalize_version_tag(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_value = *left_parts.get(index).unwrap_or(&0);
        let right_value = *right_parts.get(index).unwrap_or(&0);
        match left_value.cmp(&right_value) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn find_windows_installer_asset(release: &GithubRelease) -> Option<&GithubReleaseAsset> {
    release.assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.ends_with(".exe") && (name.contains("setup") || name.contains("x64"))
    })
}

async fn download_update_asset(
    client: &Client,
    url: &str,
    target_path: &Path,
) -> Result<(), String> {
    let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("下载安装包失败：{}", response.status()));
    }
    let temp_path = target_path.with_extension("download");
    let mut file = fs::File::create(&temp_path)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    if fs::metadata(target_path).await.is_ok() {
        let _ = fs::remove_file(target_path).await;
    }
    fs::rename(&temp_path, target_path)
        .await
        .map_err(|e| e.to_string())
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            select_lazer_dir,
            select_stable_osu_dir,
            scan_stable_collections,
            export_collection_playlist,
            import_seekman_playlist,
            apply_local_playlist_items_to_collection,
            scan_songs,
            scan_lazer,
            search_beatmapsets,
            search_alpha_recommendations,
            enqueue_downloads,
            start_downloads,
            pause_downloads,
            clear_completed,
            retry_failed_downloads,
            clear_all_downloads,
            delete_download_group,
            force_finish_download_group,
            open_api_page,
            open_external_url,
            check_for_updates,
            dismiss_update_version,
            install_update_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
