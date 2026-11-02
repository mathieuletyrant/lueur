//! `lueur serve`: the player, the Analysis queue and the HTTP API behind the
//! web UI, in one process (see docs/adr/0001-api-in-rust-binary.md).

use crate::config::Config;
use crate::hue::{self, Bridge};
use crate::jellyfin::Jellyfin;
use crate::player::{Hold, Live, Player, Status, TestPattern};
use crate::queue::{self, Queue};
use crate::track;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{info, warn};

struct App {
    path: PathBuf,
    cfg: Mutex<Config>,
    live: Arc<Live>,
    queue: Arc<Queue>,
    /// Restarts the player with the current config.
    reload: Notify,
    stopping: AtomicBool,
    last_scan: Mutex<Option<LastScan>>,
    /// Barcodes by track file, with the track's modification time.
    barcodes: Mutex<std::collections::HashMap<PathBuf, Barcode>>,
}

#[derive(Clone, Serialize)]
struct LastScan {
    /// Unix seconds.
    at: u64,
    auto: bool,
    #[serde(flatten)]
    result: queue::ScanResult,
}

/// A film's barcode, with the modification time of the track it came from.
type Barcode = (std::time::SystemTime, Arc<Vec<String>>);

struct ApiError(StatusCode, String);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(StatusCode::BAD_GATEWAY, format!("{:#}", e.into()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

type Res<T = Value> = Result<Json<T>, ApiError>;

impl App {
    fn config(&self) -> Config {
        self.cfg.lock().unwrap().clone()
    }

    /// Applies a change, saves it and pushes the live parts (Look, jobs).
    fn update(&self, f: impl FnOnce(&mut Config) -> Result<(), ApiError>, reload: bool) -> Result<Config, ApiError> {
        let cfg = {
            let mut cur = self.cfg.lock().unwrap();
            let mut next = cur.clone();
            f(&mut next)?;
            next.save(&self.path)?;
            *cur = next.clone();
            next
        };
        *self.live.look.lock().unwrap() = cfg.look.clone();
        self.live.off.store(!cfg.lights_on, Ordering::Relaxed);
        self.queue.set_jobs(cfg.jobs);
        self.queue.set_limits(cfg.analysis.clone());
        if reload {
            self.reload.notify_one();
        }
        Ok(cfg)
    }

    fn bridge(&self) -> Result<Bridge, ApiError> {
        let h = self.config().hue;
        if h.username.is_empty() {
            return Err(bad("bridge not paired"));
        }
        Ok(Bridge::new(&h))
    }

    fn jellyfin(&self) -> Result<Jellyfin, ApiError> {
        let j = self.config().jellyfin;
        if j.url.is_empty() {
            return Err(bad("Jellyfin not configured"));
        }
        Ok(Jellyfin::new(&j.url, &j.api_key, None))
    }
}

pub async fn serve(path: PathBuf, cfg: Config, port: u16) -> anyhow::Result<()> {
    let live = Arc::new(Live { look: cfg.look.clone().into(), off: (!cfg.lights_on).into(), ..Default::default() });
    let queue = Queue::new(cfg.store.clone(), crate::analyze_options(), cfg.jobs);
    queue.set_limits(cfg.analysis.clone());
    let app = Arc::new(App {
        path,
        cfg: cfg.into(),
        live,
        queue,
        reload: Notify::new(),
        stopping: AtomicBool::new(false),
        last_scan: Mutex::new(None),
        barcodes: Default::default(),
    });
    let sup = tokio::spawn(supervise(app.clone()));
    tokio::spawn(auto_scan(app.clone()));

    let router = Router::new()
        .route("/api/config", get(get_config).patch(patch_config))
        .route("/api/status", get(status))
        .route("/api/bridges", get(bridges))
        .route("/api/bridge/pair", post(pair))
        .route("/api/areas", get(areas))
        .route("/api/test", post(test))
        .route("/api/jellyfin/info", get(jellyfin_info))
        .route("/api/jellyfin/devices", get(jellyfin_devices))
        .route("/api/jellyfin/libraries", get(jellyfin_libraries))
        .route("/api/libraries", get(libraries))
        .route("/api/scan", post(scan))
        .route("/api/scan/stop", post(stop))
        .route("/api/videos", get(videos))
        .route("/api/analyze", post(analyze))
        .route("/api/frame", get(frame))
        .route("/api/barcode", get(barcode))
        .route("/api/hold", axum::routing::put(put_hold).delete(delete_hold))
        .route("/api/fs", get(fs))
        .fallback(web)
        .with_state(app.clone());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    info!("web UI on http://localhost:{port}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    app.stopping.store(true, Ordering::SeqCst);
    app.reload.notify_one();
    tokio::time::timeout(Duration::from_secs(5), sup).await.ok();
    Ok(())
}

/// Keeps a player running with the latest config; restarts it on every change.
async fn supervise(app: Arc<App>) {
    while !app.stopping.load(Ordering::SeqCst) {
        let cfg = app.config();
        let error = if cfg.hue.username.is_empty() {
            "Bridge not paired".to_string()
        } else if cfg.jellyfin.url.is_empty() {
            "Jellyfin not configured".to_string()
        } else {
            match Player::new(cfg, app.live.clone(), app.queue.clone()).await {
                Ok(p) => match p.run(app.reload.notified()).await {
                    Ok(()) => continue,
                    Err(e) => format!("{e:#}"),
                },
                Err(e) => format!("{e:#}"),
            }
        };
        *app.live.status.lock().unwrap() = Status { error: Some(error), ..Default::default() };
        // Wait for a config change, or retry an unreachable bridge.
        tokio::select! {
            _ = app.reload.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
        }
    }
}

/// Config without its secrets.
fn public(cfg: &Config) -> Value {
    let mut v = serde_json::to_value(cfg).unwrap_or_default();
    v["paired"] = json!(!cfg.hue.username.is_empty());
    v["has_api_key"] = json!(!cfg.jellyfin.api_key.is_empty());
    v["hue"]["username"] = json!("");
    v["hue"]["clientkey"] = json!("");
    v["jellyfin"]["api_key"] = json!("");
    v
}

/// JSON merge patch (RFC 7396): objects merge, `null` removes, the rest replaces.
fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                if v.is_null() {
                    a.remove(&k);
                } else {
                    merge(a.entry(k).or_insert(Value::Null), v);
                }
            }
        }
        (a, b) => *a = b,
    }
}

async fn get_config(State(app): State<Arc<App>>) -> Res {
    Ok(Json(public(&app.config())))
}

async fn patch_config(State(app): State<Arc<App>>, Json(patch): Json<Value>) -> Res {
    // The Look, the job count and the Auto scan apply live; anything else restarts the player.
    let live = ["look", "jobs", "auto_scan_minutes", "exclude", "analysis", "lights_on"];
    let reload = patch.as_object().is_some_and(|o| o.keys().any(|k| !live.contains(&k.as_str())));
    let cfg = app.update(
        |c| {
            let mut v = serde_json::to_value(&*c)?;
            merge(&mut v, patch);
            let mut next: Config = serde_json::from_value(v).map_err(|e| bad(e.to_string()))?;
            // Secrets are never sent to the UI, so an empty one means "unchanged".
            for (n, o) in [
                (&mut next.hue.username, &c.hue.username),
                (&mut next.hue.clientkey, &c.hue.clientkey),
                (&mut next.jellyfin.api_key, &c.jellyfin.api_key),
            ] {
                if n.is_empty() {
                    n.clone_from(o);
                }
            }
            *c = next;
            Ok(())
        },
        reload,
    )?;
    Ok(Json(public(&cfg)))
}

async fn status(State(app): State<Arc<App>>) -> Res {
    let player = app.live.status.lock().unwrap().clone();
    let last_scan = app.last_scan.lock().unwrap().clone();
    let hold = app.live.hold.lock().unwrap().clone();
    Ok(Json(json!({ "player": player, "queue": app.queue.snapshot(), "last_scan": last_scan, "hold": hold })))
}

async fn bridges() -> Res<Vec<hue::Found>> {
    Ok(Json(hue::discover().await))
}

#[derive(Deserialize)]
struct PairReq {
    host: String,
}

/// One pairing attempt: the UI calls it every second while the user walks to the bridge.
async fn pair(State(app): State<Arc<App>>, Json(r): Json<PairReq>) -> Res {
    let host = r.host.trim().to_string();
    if host.is_empty() {
        return Err(bad("bridge address required"));
    }
    let base = app.config().hue.base_url;
    let Some(c) = hue::pair(&host, base.as_deref()).await? else {
        return Ok(Json(json!({ "paired": false })));
    };
    app.update(
        |cfg| {
            if cfg.hue.bridge != host {
                cfg.hue.area = None;
            }
            cfg.hue.bridge = host;
            cfg.hue.username = c.username;
            cfg.hue.clientkey = c.clientkey;
            Ok(())
        },
        true,
    )?;
    Ok(Json(json!({ "paired": true })))
}

async fn areas(State(app): State<Arc<App>>) -> Res<Vec<hue::Area>> {
    Ok(Json(app.bridge()?.areas().await?))
}

#[derive(Deserialize, Default)]
struct TestReq {
    /// Blink this channel only.
    channel: Option<u8>,
}

/// Asks the player for the test pattern; a film playing takes precedence.
async fn test(State(app): State<Arc<App>>, body: Option<Json<TestReq>>) -> Res {
    let Json(r) = body.unwrap_or_default();
    app.lights_free()?;
    let start = Instant::now();
    let seconds = if r.channel.is_some() { 3 } else { 6 };
    *app.live.test.lock().unwrap() =
        Some(TestPattern { only: r.channel, start, until: start + Duration::from_secs(seconds) });
    Ok(Json(json!({})))
}

async fn jellyfin_info(State(app): State<Arc<App>>) -> Res {
    let i = app.jellyfin()?.info().await?;
    Ok(Json(json!({ "name": i["ServerName"], "version": i["Version"] })))
}

async fn jellyfin_devices(State(app): State<Arc<App>>) -> Res<Vec<String>> {
    Ok(Json(app.jellyfin()?.devices().await?))
}

async fn jellyfin_libraries(State(app): State<Arc<App>>) -> Res<Vec<crate::jellyfin::JellyfinLibrary>> {
    Ok(Json(app.jellyfin()?.libraries().await?))
}

#[derive(Serialize)]
struct LibraryStats {
    name: String,
    jellyfin: String,
    path: String,
    exists: bool,
    videos: usize,
    analyzed: usize,
}

async fn libraries(State(app): State<Arc<App>>) -> Res<Vec<LibraryStats>> {
    let cfg = app.config();
    let stats = tokio::task::spawn_blocking(move || {
        queue::library_videos(&cfg)
            .into_iter()
            .map(|(l, videos)| LibraryStats {
                name: l.name.clone(),
                jellyfin: l.jellyfin.clone(),
                path: l.path.clone(),
                exists: Path::new(&l.path).is_dir(),
                videos: videos.len(),
                analyzed: videos.iter().filter(|v| track::is_fresh(&cfg.store, v)).count(),
            })
            .collect()
    })
    .await?;
    Ok(Json(stats))
}

impl App {
    async fn scan(self: &Arc<Self>, auto: bool) -> anyhow::Result<LastScan> {
        let cfg = self.config();
        let q = self.queue.clone();
        let result = tokio::task::spawn_blocking(move || queue::scan(&cfg, &q)).await?;
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let last = LastScan { at, auto, result };
        *self.last_scan.lock().unwrap() = Some(last.clone());
        Ok(last)
    }
}

/// Runs a Scan every `auto_scan_minutes`, the first one shortly after startup.
async fn auto_scan(app: Arc<App>) {
    let mut last: Option<std::time::Instant> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        // The analysis window may have opened.
        app.queue.kick();
        let (minutes, has_libraries) = {
            let c = app.cfg.lock().unwrap();
            (c.auto_scan_minutes, !c.libraries.is_empty())
        };
        let due = last.is_none_or(|l| l.elapsed() >= Duration::from_secs(minutes as u64 * 60));
        if minutes == 0 || !has_libraries || !due {
            continue;
        }
        last = Some(std::time::Instant::now());
        match app.scan(true).await {
            Ok(s) if s.result.queued > 0 => info!("auto scan: {} file(s) queued", s.result.queued),
            Ok(_) => {}
            Err(e) => warn!("auto scan: {e:#}"),
        }
    }
}

/// "Analyze missing": a Scan right now.
async fn scan(State(app): State<Arc<App>>) -> Res<LastScan> {
    Ok(Json(app.scan(false).await?))
}

async fn stop(State(app): State<Arc<App>>) -> Res {
    app.queue.stop();
    Ok(Json(json!({})))
}

#[derive(Serialize)]
struct Video {
    library: String,
    name: String,
    /// Folder inside the Library (the show and season for an episode).
    folder: String,
    path: PathBuf,
    analyzed: bool,
    /// Seconds, when analyzed.
    duration: Option<f64>,
    /// "running" or "queued".
    state: Option<&'static str>,
}

/// Every video of every Library, with its Track and queue state.
async fn videos(State(app): State<Arc<App>>) -> Res<Vec<Video>> {
    let cfg = app.config();
    let (pending, running) = app.queue.queued();
    let list = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for (l, files) in queue::library_videos(&cfg) {
            out.extend(files.into_iter().map(|f| {
                let duration = track::fresh_duration(&cfg.store, &f);
                Video {
                    library: l.name.clone(),
                    name: f.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                    folder: f
                        .parent()
                        .and_then(|p| p.strip_prefix(&l.path).ok())
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    analyzed: duration.is_some(),
                    duration,
                    state: if running.contains(&f) {
                        Some("running")
                    } else if pending.contains(&f) {
                        Some("queued")
                    } else {
                        None
                    },
                    path: f,
                }
            }));
        }
        out
    })
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
struct AnalyzeReq {
    path: PathBuf,
}

/// Analyzes one video next, even when its Track is up to date.
async fn analyze(State(app): State<Arc<App>>, Json(r): Json<AnalyzeReq>) -> Res {
    app.library_video(&r.path)?;
    Ok(Json(json!({ "queued": app.queue.push_front(r.path) })))
}

impl App {
    /// Only files inside a Library: the API has no auth.
    fn library_video(&self, p: &Path) -> Result<(), ApiError> {
        let inside = self.config().libraries.iter().any(|l| p.starts_with(&l.path));
        if !inside || !p.is_file() || p.components().any(|c| c == std::path::Component::ParentDir) {
            return Err(bad("not a video of a Library"));
        }
        Ok(())
    }

    /// The player shows a still frame or the test pattern only when it runs
    /// and no film is playing.
    fn lights_free(&self) -> Result<(), ApiError> {
        let st = self.live.status.lock().unwrap();
        if st.channels.is_empty() {
            return Err(bad(st.error.clone().unwrap_or_else(|| "the lights aren't ready yet".into())));
        }
        if st.playing.as_ref().is_some_and(|p| p.has_track && !p.paused) {
            return Err(ApiError(StatusCode::CONFLICT, "a film is playing".into()));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct FrameQuery {
    path: PathBuf,
    t: f64,
    /// Width in pixels (160 to 1920, 960 by default).
    w: Option<u32>,
}

/// One frame of a video, as a JPEG (to pick a moment to hold on the lights).
async fn frame(State(app): State<Arc<App>>, Query(q): Query<FrameQuery>) -> Result<Response, ApiError> {
    app.library_video(&q.path)?;
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new("nice")
            .args(["-n", "19", "ffmpeg", "-v", "error", "-ss", &format!("{:.2}", q.t.max(0.0)), "-i"])
            .arg(&q.path)
            .args([
                "-map",
                "0:V:0",
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={}:-2", q.w.unwrap_or(960).clamp(160, 1920)),
            ])
            .args(["-f", "mjpeg", "pipe:1"])
            .output()
    })
    .await??;
    if out.stdout.is_empty() {
        return Err(bad(format!("no frame: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "max-age=86400")], out.stdout).into_response())
}

#[derive(Deserialize)]
struct BarcodeQuery {
    path: PathBuf,
}

/// The film's colors from start to end (see `Track::barcode`).
async fn barcode(State(app): State<Arc<App>>, Query(q): Query<BarcodeQuery>) -> Result<Response, ApiError> {
    app.library_video(&q.path)?;
    let tp = track::track_path(&app.config().store, &q.path);
    let not_yet = || ApiError(StatusCode::NOT_FOUND, "not analyzed yet".into());
    let modified = std::fs::metadata(&tp).and_then(|m| m.modified()).map_err(|_| not_yet())?;
    // A track decompresses to tens of MB: compute each barcode once.
    let cached = app.barcodes.lock().unwrap().get(&tp).filter(|c| c.0 == modified).map(|c| c.1.clone());
    let hex = match cached {
        Some(h) => h,
        None => {
            let t = tp.clone();
            let bar = tokio::task::spawn_blocking(move || track::Track::load(&t).map(|t| t.barcode(240)))
                .await?
                .map_err(|_| not_yet())?;
            let h: Arc<Vec<String>> =
                Arc::new(bar.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect());
            app.barcodes.lock().unwrap().insert(tp, (modified, h.clone()));
            h
        }
    };
    let hex = (*hex).clone();
    // The URL carries the track's duration: a new analysis gets a new URL.
    Ok(([(header::CACHE_CONTROL, "max-age=86400")], Json(hex)).into_response())
}

#[derive(Deserialize)]
struct HoldReq {
    path: PathBuf,
    t: f64,
}

/// Holds a moment of a video on the lights (or moves it) until DELETE, a
/// film starting, or 10 minutes.
async fn put_hold(State(app): State<Arc<App>>, Json(r): Json<HoldReq>) -> Res {
    app.library_video(&r.path)?;
    if !track::is_fresh(&app.config().store, &r.path) {
        return Err(bad("analyze this video first"));
    }
    app.lights_free()?;
    let until = Instant::now() + Duration::from_secs(600);
    *app.live.hold.lock().unwrap() = Some(Hold { path: r.path, t: r.t, until });
    Ok(Json(json!({})))
}

async fn delete_hold(State(app): State<Arc<App>>) -> Res {
    *app.live.hold.lock().unwrap() = None;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
struct FsQuery {
    path: Option<String>,
}

/// Sub-folders of `path`, or of its closest existing parent.
async fn fs(Query(q): Query<FsQuery>) -> Res {
    let mut p = PathBuf::from(q.path.filter(|p| !p.is_empty()).unwrap_or_else(|| "/".into()));
    while !p.is_dir() {
        if !p.pop() {
            p = PathBuf::from("/");
            break;
        }
    }
    let mut dirs: Vec<String> = std::fs::read_dir(&p)
        .map_err(|e| bad(format!("{}: {e}", p.display())))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    dirs.sort_by_key(|d| d.to_lowercase());
    Ok(Json(json!({ "path": p, "parent": p.parent(), "dirs": dirs })))
}

#[derive(rust_embed::Embed)]
#[folder = "../web/dist"]
#[allow_missing = true]
struct Web;

/// The built UI; unknown paths get `index.html` (client-side views).
async fn web(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    match Web::get(path).or_else(|| Web::get("index.html")) {
        Some(f) => ([(header::CONTENT_TYPE, f.metadata.mimetype().to_string())], f.data).into_response(),
        None => (StatusCode::NOT_FOUND, "UI not built: run `pnpm build` first").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_patch() {
        let mut a = json!({ "hue": { "bridge": "1.2.3.4", "area": "x" }, "jobs": 1 });
        merge(&mut a, json!({ "hue": { "area": null }, "jobs": 2 }));
        assert_eq!(a, json!({ "hue": { "bridge": "1.2.3.4" }, "jobs": 2 }));
    }
}
