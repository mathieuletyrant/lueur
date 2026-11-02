mod analyze;
mod color;
mod config;
mod hue;
mod jellyfin;
mod player;
mod track;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Synchronise des lampes Philips Hue avec les films lus sur Jellyfin.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Fichier de configuration.
    #[arg(short, long, env = "LUEUR_CONFIG", default_value = "lueur.json", global = true)]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Analyse des films (fichiers ou dossiers) et écrit leurs pistes de couleurs.
    Analyze {
        paths: Vec<PathBuf>,
        /// Images analysées par seconde.
        #[arg(long, default_value_t = 12.0)]
        fps: f32,
        #[arg(long, value_enum, default_value_t = analyze::HwAccel::Auto)]
        hwaccel: analyze::HwAccel,
        /// Analyses en parallèle.
        #[arg(short, long, default_value_t = 1)]
        jobs: usize,
        /// Réanalyse même si une piste à jour existe.
        #[arg(long)]
        force: bool,
    },
    /// Affiche une piste (et la grille à un instant donné).
    Inspect {
        track: PathBuf,
        /// Instant en secondes.
        #[arg(long)]
        at: Option<f64>,
    },
    /// Appaire lueur avec le pont (appuyez sur son bouton).
    Pair {
        /// IP du pont.
        bridge: String,
    },
    /// Liste les zones de divertissement du pont.
    Areas,
    /// Envoie un arc-en-ciel sur la zone pour vérifier la configuration.
    Test {
        #[arg(long, default_value_t = 10.0)]
        seconds: f32,
    },
    /// Suit Jellyfin et pilote les lampes.
    Run,
}

const VIDEO_EXT: &[&str] = &["mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "webm"];

fn collect(paths: &[PathBuf], out: &mut Vec<PathBuf>) {
    for p in paths {
        if p.is_dir() {
            let Ok(rd) = std::fs::read_dir(p) else { continue };
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

fn cmd_analyze(store: &Path, paths: &[PathBuf], opts: analyze::Options, jobs: usize, force: bool) -> Result<()> {
    let mut files = Vec::new();
    collect(paths, &mut files);
    let todo: Vec<_> = files.into_iter().filter(|f| force || !track::is_fresh(store, f)).collect();
    if todo.is_empty() {
        println!("Rien à analyser, toutes les pistes sont à jour.");
        return Ok(());
    }
    let total = todo.len();
    let queue = Arc::new(Mutex::new(todo));
    let done = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = (0..jobs.max(1))
        .map(|_| {
            let (queue, done, failed, store, opts) = (queue.clone(), done.clone(), failed.clone(), store.to_path_buf(), opts.clone());
            std::thread::spawn(move || loop {
                let Some(f) = queue.lock().unwrap().pop() else { break };
                let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                match analyze::analyze(&f, &opts).and_then(|(t, s)| t.save(&track::track_path(&store, &f)).map(|_| (t, s))) {
                    Ok((t, s)) => {
                        let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                        let speed = t.duration() / s.elapsed;
                        println!(
                            "[{n}/{total}] {name} : {:.0} min de film en {:.1} s (×{speed:.0} temps réel, {:?})",
                            t.duration() / 60.0,
                            s.elapsed,
                            s.hwaccel
                        );
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        eprintln!("échec {name} : {e:#}");
                    }
                }
            })
        })
        .collect();
    for w in workers {
        w.join().ok();
    }
    let f = failed.load(Ordering::SeqCst);
    if f > 0 {
        bail!("{f} analyse(s) en échec");
    }
    Ok(())
}

fn cmd_inspect(path: &Path, at: Option<f64>) -> Result<()> {
    let t = track::Track::load(path)?;
    let size = std::fs::metadata(path)?.len();
    println!("source   : {}", t.source);
    println!("durée    : {:.1} min, {} images à {} i/s, grille {}×{}", t.duration() / 60.0, t.frame_count(), t.fps, t.cols, t.rows);
    println!("fichier  : {:.1} Ko", size as f64 / 1024.0);
    if let Some(at) = at {
        let mut g = Vec::new();
        t.sample(at, &mut g);
        println!("grille à {at:.2} s :");
        for r in 0..t.rows as usize {
            let line: String = (0..t.cols as usize)
                .map(|c| {
                    let px = g[r * t.cols as usize + c].map(|v| (color::linear_to_srgb(v) * 255.0).round() as u8);
                    format!("\x1b[48;2;{};{};{}m  ", px[0], px[1], px[2])
                })
                .collect();
            println!("  {line}\x1b[0m");
        }
        let mid = g[(t.rows as usize / 2) * t.cols as usize + t.cols as usize / 2].map(|v| (color::linear_to_srgb(v) * 255.0).round() as u8);
        println!("centre   : rgb({}, {}, {})", mid[0], mid[1], mid[2]);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "lueur=info".into()))
        .with_target(false)
        .init();
    let cli = Cli::parse();
    let mut cfg = config::Config::load(&cli.config)?;
    match cli.cmd {
        Cmd::Analyze { paths, fps, hwaccel, jobs, force } => {
            let opts = analyze::Options { fps, cols: 16, rows: 9, hwaccel, threads: 0 };
            let store = cfg.store.clone();
            tokio::task::spawn_blocking(move || cmd_analyze(&store, &paths, opts, jobs, force)).await??;
        }
        Cmd::Inspect { track, at } => cmd_inspect(&track, at)?,
        Cmd::Pair { bridge } => {
            println!("Appuyez sur le bouton du pont {bridge} (60 s)…");
            let base = cfg.hue.base_url.clone();
            for _ in 0..60 {
                if let Some(c) = hue::pair(&bridge, base.as_deref()).await? {
                    cfg.hue.bridge = bridge;
                    cfg.hue.username = c.username;
                    cfg.hue.clientkey = c.clientkey;
                    cfg.save(&cli.config)?;
                    println!("Appairé, identifiants enregistrés dans {}.", cli.config.display());
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            bail!("bouton non pressé à temps");
        }
        Cmd::Areas => {
            let creds = hue::Credentials { username: cfg.hue.username.clone(), clientkey: cfg.hue.clientkey.clone() };
            let b = hue::Bridge::new(&cfg.hue.bridge, creds, cfg.hue.base_url.as_deref());
            for a in b.areas().await? {
                println!("{} — {} ({}, {})", a.metadata.name, a.id, a.configuration_type, a.status);
                for ch in &a.channels {
                    println!("   canal {} : x={:+.2} y={:+.2} z={:+.2}", ch.channel_id, ch.position.x, ch.position.y, ch.position.z);
                }
            }
        }
        Cmd::Test { seconds } => player::test_pattern(&cfg, seconds).await?,
        Cmd::Run => player::Player::new(cfg).await?.run().await?,
    }
    Ok(())
}
