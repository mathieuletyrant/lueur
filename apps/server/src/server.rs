//! `lueur serve`: the player, the Analysis queue and the HTTP API behind the
//! web UI, in one process (see docs/adr/0001-api-in-rust-binary.md).

use crate::config::Config;
use crate::hue::{self, Bridge, Credentials};
use crate::jellyfin::Jellyfin;
use crate::player::{self, Live, Player, Status};
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
use std::time::Duration;
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
}

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
        self.queue.set_jobs(cfg.jobs);
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
        Ok(Bridge::new(&h.bridge, Credentials { username: h.username, clientkey: h.clientkey }, h.base_url.as_deref()))
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
    let live = Arc::new(Live { look: cfg.look.clone().into(), ..Default::default() });
    let queue = Queue::new(cfg.store.clone(), crate::analyze_options(), cfg.jobs);
    let app =
        Arc::new(App { path, cfg: cfg.into(), live, queue, reload: Notify::new(), stopping: AtomicBool::new(false) });
    let sup = tokio::spawn(supervise(app.clone()));

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
    // The Look and the job count apply live; anything else restarts the player.
    let reload = patch.as_object().is_some_and(|o| o.keys().any(|k| k != "look" && k != "jobs"));
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
    Ok(Json(json!({ "player": player, "queue": app.queue.snapshot() })))
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

async fn test(State(app): State<Arc<App>>) -> Res {
    if app.live.status.lock().unwrap().streaming {
        return Err(ApiError(StatusCode::CONFLICT, "a film is playing".into()));
    }
    app.bridge()?;
    let cfg = app.config();
    // ponytail: no lock against the player, a film starting during the 6 s test fights over the stream.
    tokio::spawn(async move {
        if let Err(e) = player::test_pattern(&cfg, 6.0).await {
            warn!("test pattern: {e:#}");
        }
    });
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
        cfg.libraries
            .iter()
            .map(|l| {
                let mut videos = Vec::new();
                queue::collect(&[PathBuf::from(&l.path)], &mut videos);
                LibraryStats {
                    name: l.name.clone(),
                    jellyfin: l.jellyfin.clone(),
                    path: l.path.clone(),
                    exists: Path::new(&l.path).is_dir(),
                    videos: videos.len(),
                    analyzed: videos.iter().filter(|v| track::is_fresh(&cfg.store, v)).count(),
                }
            })
            .collect()
    })
    .await?;
    Ok(Json(stats))
}

/// Queues an Analysis for every video of every Library without an up-to-date Track.
async fn scan(State(app): State<Arc<App>>) -> Res {
    let cfg = app.config();
    let q = app.queue.clone();
    let queued = tokio::task::spawn_blocking(move || {
        let roots: Vec<PathBuf> = cfg.libraries.iter().map(|l| PathBuf::from(&l.path)).collect();
        let mut videos = Vec::new();
        queue::collect(&roots, &mut videos);
        videos.into_iter().filter(|v| !track::is_fresh(&cfg.store, v)).filter(|v| q.push(v.clone())).count()
    })
    .await?;
    Ok(Json(json!({ "queued": queued })))
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
