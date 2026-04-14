use altered_history::classes;
use altered_history::db;
use altered_history::env;
use flexi_logger::{Cleanup, Criterion, Logger, Naming};
use log::info;
use log::LevelFilter;
use std::sync::atomic::Ordering;
use std::time::Instant;

const GRAPH_PATH: &str = "/poolswh/softwareheritage/graph/2025-05-18/compressed/graph";
const RESULTS_PATH: &str = "/home/infres/rapaport/results/FULL_2025_05_18"; // kept for log directory

fn main() {
    let opts = altered_history::env::Options {
        graph: String::from(GRAPH_PATH),
        results: String::from(RESULTS_PATH),
        expected_origins: 373_130_808,
        chunk: 50_000,
        removed_branch: false,
    };

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                .directory(format!("{}/logs", opts.results))
                .basename("FULL_2025_05")
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
    rt.block_on(db::create_tables(&pool))
        .expect("Failed to create DB tables");

    println!("START");

    // Step 1: Retrieve History Alterations
    let start = Instant::now();
    let cp = rt
        .block_on(db::load_checkpoints(&pool))
        .ok();
    altered_history::main_all_mpsc_with_cp(&opts, cp, &pool, &rt);

    println!(
        "Origins rejected: {}",
        env::ORI_REJECTED.load(Ordering::Relaxed)
    );
    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());

    // Step 2: Identify Root Cause Commits
    let start = Instant::now();
    if altered_history::analysis::focus_missing_commits_from_db(&opts, &pool, &rt) {
        info!("Focus finished!");
    }
    println!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    info!("Focus complete | time elapsed: {:.2?}", start.elapsed());

    // Step 3: Categorize Root Cause Commits
    let start = Instant::now();
    classes::categorization_all_from_db(&opts, &pool, &rt);
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
