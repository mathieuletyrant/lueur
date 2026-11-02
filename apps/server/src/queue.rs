//! Background Analysis queue, shared by the CLI, the player and the API.
//! Each running Analysis gets its own thread; at most `jobs` run at once.

use crate::analyze;
use crate::config::{Config, Library};
use crate::track;
use serde::Serialize;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;
use tracing::{info, warn};

const VIDEO_EXT: &[&str] = &["mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "webm"];
const KEEP_FAILED: usize = 50;

/// Video files under `paths` (files or folders, recursively), sorted.
pub fn collect(paths: &[PathBuf], out: &mut Vec<PathBuf>) {
    for p in paths {
        if p.is_dir() {
            let Ok(rd) = std::fs::read_dir(p) else {
                continue;
            };
            let mut children: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            children.sort();
            collect(&children, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| VIDEO_EXT.contains(&e.to_ascii_lowercase().as_str()))
        {
            out.push(p.clone());
        }
    }
}

/// Files changed more recently than this are still being copied: Scans skip them.
const SETTLE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanResult {
    pub queued: usize,
    /// Orphan Tracks deleted.
    pub removed: usize,
}

/// Queues an Analysis for every video of `libraries` without an up-to-date
/// Track, and deletes the Orphan Tracks: those of videos that are gone,
/// excluded or outside the Libraries. Deletes nothing when a Library can't be read (an
/// unmounted disk would otherwise wipe the Store).
pub fn scan(cfg: &Config, q: &Arc<Queue>) -> ScanResult {
    let store = cfg.store.as_path();
    let videos: Vec<PathBuf> = library_videos(cfg).into_iter().flat_map(|(_, v)| v).collect();
    let settled = |v: &PathBuf| {
        std::fs::metadata(v).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e >= SETTLE))
    };
    let mut r = ScanResult::default();
    for v in &videos {
        if !track::is_fresh(store, v) && settled(v) && q.push(v.clone()) {
            r.queued += 1;
        }
    }
    if cfg.libraries.is_empty() || !cfg.libraries.iter().all(|l| Path::new(&l.path).is_dir()) {
        return r;
    }
    let keep: HashSet<PathBuf> = videos.iter().map(|v| track::track_path(store, v)).collect();
    for e in std::fs::read_dir(store).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "lueur") && !keep.contains(&p) && std::fs::remove_file(&p).is_ok() {
            r.removed += 1;
        }
    }
    if r.removed > 0 {
        info!("removed {} orphan track(s)", r.removed);
    }
    r
}

/// Each Library with its videos, excluded ones left out.
pub fn library_videos(cfg: &Config) -> Vec<(&Library, Vec<PathBuf>)> {
    cfg.libraries
        .iter()
        .map(|l| {
            let mut videos = Vec::new();
            collect(&[PathBuf::from(&l.path)], &mut videos);
            videos.retain(|v| !excluded(v, &cfg.exclude));
            (l, videos)
        })
        .collect()
}

/// The path contains one of `patterns`, ignoring case.
pub fn excluded(video: &Path, patterns: &[String]) -> bool {
    let p = video.to_string_lossy().to_lowercase();
    patterns.iter().any(|x| !x.trim().is_empty() && p.contains(&x.trim().to_lowercase()))
}

struct Job {
    video: PathBuf,
    /// Seconds of video, when ffprobe knows it (probed by the job's thread).
    duration: OnceLock<Option<f64>>,
    frames: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    on_cpu: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    pending: VecDeque<PathBuf>,
    running: Vec<Arc<Job>>,
    done: usize,
    failed: VecDeque<Failed>,
}

#[derive(Clone, Serialize)]
pub struct Failed {
    pub name: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct Running {
    pub name: String,
    /// 0..1, absent when the duration is unknown.
    pub progress: Option<f64>,
    /// Decoding on the CPU: slower, and heavier on the server.
    pub cpu: bool,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub jobs: usize,
    pub running: Vec<Running>,
    pub pending: usize,
    pub next: Vec<String>,
    pub done: usize,
    pub failed: Vec<Failed>,
}

pub struct Queue {
    store: PathBuf,
    opts: analyze::Options,
    jobs: AtomicUsize,
    state: Mutex<State>,
    idle: Condvar,
}

fn name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

impl Queue {
    pub fn new(store: PathBuf, opts: analyze::Options, jobs: usize) -> Arc<Self> {
        Arc::new(Self {
            store,
            opts,
            jobs: AtomicUsize::new(jobs.max(1)),
            state: Default::default(),
            idle: Condvar::new(),
        })
    }

    pub fn set_jobs(self: &Arc<Self>, jobs: usize) {
        self.jobs.store(jobs.max(1), Ordering::SeqCst);
        self.pump(&mut self.state.lock().unwrap());
    }

    /// Queues a video unless it is already queued or running. Returns whether it was added.
    pub fn push(self: &Arc<Self>, video: PathBuf) -> bool {
        self.add(video, false)
    }

    /// Like `push`, but ahead of everything already waiting.
    pub fn push_front(self: &Arc<Self>, video: PathBuf) -> bool {
        self.add(video, true)
    }

    fn add(self: &Arc<Self>, video: PathBuf, front: bool) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.running.iter().any(|j| j.video == video) {
            return false;
        }
        if let Some(i) = s.pending.iter().position(|p| *p == video) {
            if !front {
                return false;
            }
            s.pending.remove(i);
        }
        if front {
            s.pending.push_front(video);
        } else {
            s.pending.push_back(video);
        }
        self.pump(&mut s);
        true
    }

    /// Drops everything waiting and stops the running analyses.
    pub fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.pending.clear();
        for j in &s.running {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Videos waiting and running.
    pub fn queued(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
        let s = self.state.lock().unwrap();
        (s.pending.iter().cloned().collect(), s.running.iter().map(|j| j.video.clone()).collect())
    }

    fn pump(self: &Arc<Self>, s: &mut State) {
        while s.running.len() < self.jobs.load(Ordering::SeqCst) {
            let Some(video) = s.pending.pop_front() else {
                break;
            };
            let job = Arc::new(Job {
                video,
                duration: OnceLock::new(),
                frames: Default::default(),
                cancel: Default::default(),
                on_cpu: Default::default(),
            });
            s.running.push(job.clone());
            let q = self.clone();
            std::thread::spawn(move || q.work(job));
        }
    }

    fn work(self: Arc<Self>, job: Arc<Job>) {
        job.duration.get_or_init(|| analyze::duration(&job.video));
        let opts = analyze::Options {
            progress: Some(job.frames.clone()),
            cancel: Some(job.cancel.clone()),
            on_cpu: Some(job.on_cpu.clone()),
            ..self.opts.clone()
        };
        let n = name(&job.video);
        let res = analyze::analyze(&job.video, &opts)
            .and_then(|(t, s)| t.save(&track::track_path(&self.store, &job.video)).map(|_| (t, s)));
        let mut s = self.state.lock().unwrap();
        s.running.retain(|j| !Arc::ptr_eq(j, &job));
        match res {
            Ok((t, st)) => {
                s.done += 1;
                info!(
                    "analyzed {n}: {:.0} min of film in {:.1} s ({:.0}× real time, {:?})",
                    t.duration() / 60.0,
                    st.elapsed,
                    t.duration() / st.elapsed,
                    st.hwaccel
                );
            }
            Err(_) if job.cancel.load(Ordering::Relaxed) => info!("analysis of {n} stopped"),
            Err(e) => {
                warn!("analysis failed for {n}: {e:#}");
                s.failed.push_front(Failed { name: n, error: format!("{e:#}") });
                s.failed.truncate(KEEP_FAILED);
            }
        }
        self.pump(&mut s);
        if s.running.is_empty() {
            self.idle.notify_all();
        }
    }

    /// Blocks until nothing is queued or running.
    pub fn wait_idle(&self) {
        let s = self.state.lock().unwrap();
        drop(self.idle.wait_while(s, |s| !s.running.is_empty() || !s.pending.is_empty()).unwrap());
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = self.state.lock().unwrap();
        Snapshot {
            jobs: self.jobs.load(Ordering::SeqCst),
            running: s
                .running
                .iter()
                .map(|j| Running {
                    name: name(&j.video),
                    cpu: j.on_cpu.load(Ordering::Relaxed),
                    progress: j
                        .duration
                        .get()
                        .copied()
                        .flatten()
                        .filter(|d| *d > 0.0)
                        .map(|d| (j.frames.load(Ordering::Relaxed) as f64 / self.opts.fps as f64 / d).min(1.0)),
                })
                .collect(),
            pending: s.pending.len(),
            next: s.pending.iter().take(5).map(|p| name(p)).collect(),
            done: s.done,
            failed: s.failed.iter().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_keeps_library_tracks_and_removes_orphans() {
        let root = std::env::temp_dir().join(format!("lueur-scan-{}", std::process::id()));
        let (lib, store) = (root.join("movies"), root.join("tracks"));
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&store).unwrap();
        let film = lib.join("Film.mkv");
        std::fs::write(&film, b"x").unwrap();
        let kept = track::track_path(&store, &film);
        let orphan = store.join("Gone-0000000000000000.lueur");
        std::fs::write(&kept, b"").unwrap();
        std::fs::write(&orphan, b"").unwrap();
        let q = Queue::new(store.clone(), crate::analyze_options(), 1);
        let library =
            |p: &Path| Library { name: String::new(), jellyfin: String::new(), path: p.to_string_lossy().into() };
        let cfg = |libs: Vec<Library>, exclude: Vec<String>| Config {
            store: store.clone(),
            libraries: libs,
            exclude,
            ..Default::default()
        };

        // A Library that can't be read: nothing is deleted.
        let r = scan(&cfg(vec![library(&lib), library(&root.join("unmounted"))], vec![]), &q);
        assert_eq!(r.removed, 0);
        assert!(orphan.exists());

        // The film was just written: not queued yet; the orphan goes, the film's Track stays.
        let r = scan(&cfg(vec![library(&lib)], vec![]), &q);
        assert_eq!((r.queued, r.removed), (0, 1));
        assert!(kept.exists() && !orphan.exists());

        // Excluded: its Track becomes an orphan too.
        let r = scan(&cfg(vec![library(&lib)], vec!["FILM".into()]), &q);
        assert_eq!(r.removed, 1);
        assert!(!kept.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn missing_file_fails_and_queue_goes_idle() {
        let dir = std::env::temp_dir().join(format!("lueur-queue-{}", std::process::id()));
        let opts = analyze::Options { hwaccel: analyze::HwAccel::None, ..crate::analyze_options() };
        let q = Queue::new(dir.clone(), opts.clone(), 1);
        assert!(q.push("/nope/a.mkv".into()));
        q.push("/nope/b.mkv".into());
        q.wait_idle();
        let s = q.snapshot();
        assert_eq!((s.done, s.failed.len(), s.pending), (0, 2, 0));

        // `stop` drops what waits; `push_front` jumps ahead.
        let q = Queue::new(dir, opts, 1);
        for f in ["/nope/c.mkv", "/nope/d.mkv", "/nope/e.mkv"] {
            q.push(f.into());
        }
        q.push_front("/nope/f.mkv".into());
        assert!(q.state.lock().unwrap().pending.front().is_some_and(|p| p.ends_with("f.mkv")));
        q.stop();
        assert!(q.queued().0.is_empty());
        q.wait_idle();
    }
}
