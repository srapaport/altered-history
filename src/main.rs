use altered_history::classes;
use altered_history::db;
use altered_history::env;
use flexi_logger::{Cleanup, Criterion, Logger, Naming};
use log::info;
use log::LevelFilter;
use std::sync::atomic::Ordering;
use std::time::Instant;

const GRAPH_PATH: &str = "/dev/shm/swh-graph/current/graph";// /path/to/compressed/graph --> keep the name of the graph (often graph) without any extension
const RESULTS_PATH: &str = "/home/infres/rapaport/results/FULL_2025_05_18";// /path/to/results/directory
const EXPECTED_ORIGINS: usize = 373_130_808;
const GRAPH_PATH_TEASE: &str = "/home/infres/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph";
const RESULTS_PATH_SINGLE: &str = "/home/infres/rapaport/results/bahamut";
const RESULTS_PATH_TEASE: &str = "/home/infres/rapaport/results/TEASE_2024_08_23_new";
const EXPECTED_ORIGINS_TEASE: usize = 443;
const NEW_GRAPH: &str = "/swh/scratch/graph/2026-03-02/compressed/graph";
const NEW_ORIGINS: usize = 429_613_456;
const RESULTS_NEW: &str = "/swh/scratch/rapaport/results/altered-history";

fn main() {
    let opts = altered_history::env::Options {
        graph: String::from(GRAPH_PATH),
        results: String::from(RESULTS_NEW),
        expected_origins: NEW_ORIGINS,
        chunk: 50_000, // arbitrary
        removed_branch: false, // not use yet --> detect when a branch was deleted
    };

    let tables = db::TableNames::from_graph_path(NEW_GRAPH);
    println!("Using tables: {:?}", tables);

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                .directory(format!("{}/logs", opts.results))
                .basename("2026_03_02")
                .suffix("log"),
        )
        .rotate(
            Criterion::Size(10_000_000),
            Naming::Numbers,
            Cleanup::KeepLogFiles(5),
        )
        .start()
        .unwrap();

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let pool = rt
        .block_on(db::init_pool())
        .expect("Failed to initialize DB pool");
    rt.block_on(db::create_tables(&pool, &tables))
        .expect("Failed to create DB tables");

    println!("START");

    // Step 1: Retrieve History Alterations
    let start = Instant::now();
    let cp = rt.block_on(db::load_checkpoints(&pool, &tables)).ok();
    altered_history::main_all_mpsc_with_cp(&opts, cp, &pool, &rt, &tables);

    println!(
        "Origins rejected: {}",
        env::ORI_REJECTED.load(Ordering::Relaxed)
    );
    info!(
        "Origins rejected: {}",
        env::ORI_REJECTED.load(Ordering::Relaxed)
    );
    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());

    // Step 2: Identify Root Cause Commits
    let start = Instant::now();
    if altered_history::analysis::focus_missing_commits_from_db(&opts, &pool, &rt, &tables) {
        info!("Focus finished!");
    }
    println!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    info!("Focus complete | time elapsed: {:.2?}", start.elapsed());

    // Step 3: Categorize Root Cause Commits
    let start = Instant::now();
    classes::categorization_all_from_db(&opts, &pool, &rt, &tables);
    println!(
        "Classification complete | time elapsed: {:.2?}",
        start.elapsed()
    );
    info!(
        "Classification complete | time elapsed: {:.2?}",
        start.elapsed()
    );
    info!(
        "Rev without a dir: {}",
        env::REV_WITHOUT_DIR.load(Ordering::Relaxed)
    );
    println!(
        "Rev without a dir: {}",
        env::REV_WITHOUT_DIR.load(Ordering::Relaxed)
    );
}
