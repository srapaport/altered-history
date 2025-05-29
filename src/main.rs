use altered_history::classes;
use altered_history::env;
use flexi_logger::{Cleanup, Criterion, Logger, Naming};
// use flexi_logger::Duplicate;
use log::info;
use log::LevelFilter;
use std::sync::atomic::Ordering;
use std::time::Instant;

const GRAPH_PATH: &str = "";// /path/to/compressed/graph --> keep the name of the graph (often graph) without any extension
const RESULTS_PATH: &str = "";// /path/to/results/directory

fn main() {
    //let opts = altered_history::env::Options::parse();
    let opts = altered_history::env::Options {
        graph: String::from(GRAPH_PATH),
        results: String::from(RESULTS_PATH),
        expected_origins: 310_334_314, // for 2024-08-23 extraction
        chunk: 50_000, // arbitrary
        removed_branch: false, // not use yet --> detect when a branch was deleted
    };

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                .directory(format!("{}/logs", opts.results))
                .basename("FULL_2024")
                .suffix("log"),
        )
        .rotate(
            Criterion::Size(10_000_000), // Rotate when the file exceeds 10 MB
            Naming::Numbers,             // Use numbers for rotated files
            Cleanup::KeepLogFiles(5),    // Keep at most 5 log files
        )
        //.duplicate_to_stderr(Duplicate::Warn) // Duplicate logs to stderr
        .start()
        .unwrap();

    println!("START");
    // Retrieve History Alterations
    let start = Instant::now();

    let cp = altered_history::load_checkpoint(&opts);
    altered_history::main_all_mpsc_with_cp(&opts, cp);

    println!(
        "Origins rejected: {}",
        env::ORI_REJECTED.load(Ordering::Relaxed)
    );
    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());

    //Identify Root Cause Commits
    let start = Instant::now();
    if altered_history::analysis::focus_missing_commits_all_files_with_save(&opts) {
        info!("Finished!");
    }
    println!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    info!("Focus complete | time elapsed: {:.2?}", start.elapsed());

    //Categorize Root Cause Commits
    let start = Instant::now();
    classes::categorization_all(&opts);
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
