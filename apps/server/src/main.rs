mod analyze;
mod color;
mod config;
mod hue;
mod jellyfin;
mod player;
mod queue;
mod server;
mod track;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Syncs Philips Hue lights with the films played on Jellyfin.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Configuration file.
    #[arg(short, long, env = "LUEUR_CONFIG", default_value = "lueur.json", global = true)]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Analyzes films (files or folders) and writes their color tracks.
    Analyze {
        paths: Vec<PathBuf>,
        /// Frames analyzed per second.
        #[arg(long, default_value_t = 12.0)]
        fps: f32,
        #[arg(long, value_enum, default_value_t = analyze::HwAccel::Auto)]
        hwaccel: analyze::HwAccel,
        /// Parallel analyses.
        #[arg(short, long, default_value_t = 1)]
        jobs: usize,
        /// Re-analyze even when an up-to-date track exists.
        #[arg(long)]
        force: bool,
    },
    /// Shows a track (and its grid at a given time).
    Inspect {
        track: PathBuf,
        /// Time in seconds.
        #[arg(long)]
        at: Option<f64>,
    },
    /// Pairs lueur with the bridge (press its link button).
    Pair {
        /// Bridge IP.
        bridge: String,
    },
    /// Lists the bridge's entertainment areas.
    Areas,
    /// Sends a rainbow to the area to check the setup.
    Test {
        #[arg(long, default_value_t = 10.0)]
        seconds: f32,
    },
    /// Follows Jellyfin and drives the lights.
    Run,
    /// Like `run`, plus the web UI and its API.
    Serve {
        #[arg(long, env = "LUEUR_PORT", default_value_t = 8080)]
        port: u16,
    },
}

pub fn analyze_options() -> analyze::Options {
    analyze::Options {
        fps: 12.0,
        cols: 16,
        rows: 9,
        hwaccel: analyze::HwAccel::Auto,
        // Software decoding only: leaves the other cores to the server.
        threads: 2,
        progress: None,
        cancel: None,
        on_cpu: None,
    }
}

fn cmd_analyze(store: &Path, paths: &[PathBuf], opts: analyze::Options, jobs: usize, force: bool) -> Result<()> {
    let mut files = Vec::new();
    queue::collect(paths, &mut files);
    let todo: Vec<_> = files.into_iter().filter(|f| force || !track::is_fresh(store, f)).collect();
    if todo.is_empty() {
        println!("Nothing to analyze, all tracks are up to date.");
        return Ok(());
    }
    println!("Analyzing {} file(s)…", todo.len());
    let q = queue::Queue::new(store.to_path_buf(), opts, jobs);
    for f in todo {
        q.push(f);
    }
    q.wait_idle();
    let f = q.snapshot().failed.len();
    if f > 0 {
        bail!("{f} analysis(es) failed");
    }
    Ok(())
}

fn cmd_inspect(path: &Path, at: Option<f64>) -> Result<()> {
    let t = track::Track::load(path)?;
    let size = std::fs::metadata(path)?.len();
    println!("source   : {}", t.source);
    println!(
        "duration : {:.1} min, {} frames at {} fps, {}×{} grid",
        t.duration() / 60.0,
        t.frame_count(),
        t.fps,
        t.cols,
        t.rows
    );
    println!("file     : {:.1} KB", size as f64 / 1024.0);
    if let Some(at) = at {
        let mut g = Vec::new();
        t.sample(at, &mut g);
        println!("grid at {at:.2} s:");
        for r in 0..t.rows as usize {
            let line: String = (0..t.cols as usize)
                .map(|c| {
                    let px = g[r * t.cols as usize + c].map(|v| (color::linear_to_srgb(v) * 255.0).round() as u8);
                    format!("\x1b[48;2;{};{};{}m  ", px[0], px[1], px[2])
                })
                .collect();
            println!("  {line}\x1b[0m");
        }
        let mid = g[(t.rows as usize / 2) * t.cols as usize + t.cols as usize / 2]
            .map(|v| (color::linear_to_srgb(v) * 255.0).round() as u8);
        println!("center   : rgb({}, {}, {})", mid[0], mid[1], mid[2]);
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
            let opts = analyze::Options { fps, hwaccel, ..analyze_options() };
            let store = cfg.store.clone();
            tokio::task::spawn_blocking(move || cmd_analyze(&store, &paths, opts, jobs, force)).await??;
        }
        Cmd::Inspect { track, at } => cmd_inspect(&track, at)?,
        Cmd::Pair { bridge } => {
            println!("Press the link button on bridge {bridge} (60 s)…");
            let base = cfg.hue.base_url.clone();
            for _ in 0..60 {
                if let Some(c) = hue::pair(&bridge, base.as_deref()).await? {
                    cfg.hue.bridge = bridge;
                    cfg.hue.username = c.username;
                    cfg.hue.clientkey = c.clientkey;
                    cfg.save(&cli.config)?;
                    println!("Paired, credentials saved to {}.", cli.config.display());
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            bail!("link button not pressed in time");
        }
        Cmd::Areas => {
            let b = hue::Bridge::new(&cfg.hue);
            for a in b.areas().await? {
                println!("{} — {} ({}, {})", a.metadata.name, a.id, a.configuration_type, a.status);
                for ch in &a.channels {
                    println!(
                        "   channel {}: x={:+.2} y={:+.2} z={:+.2}",
                        ch.channel_id, ch.position.x, ch.position.y, ch.position.z
                    );
                }
            }
        }
        Cmd::Test { seconds } => player::test_pattern(&cfg, seconds, None).await?,
        Cmd::Run => {
            let live = Arc::new(player::Live { look: cfg.look.clone().into(), ..Default::default() });
            let q = queue::Queue::new(cfg.store.clone(), analyze_options(), cfg.jobs);
            let stop = async {
                tokio::signal::ctrl_c().await.ok();
            };
            player::Player::new(cfg, live, q).await?.run(stop).await?
        }
        Cmd::Serve { port } => server::serve(cli.config, cfg, port).await?,
    }
    Ok(())
}
