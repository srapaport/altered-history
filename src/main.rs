use altered_history::classes;
use altered_history::env;
use clap::Parser;
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::info;
use log::LevelFilter;
use std::sync::atomic::Ordering;
use std::time::Instant;

fn main() {
    let opts = altered_history::env::Options::parse();

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                .basename("FULL-all")
                .suffix("log"),
        )
        .rotate(
            Criterion::Size(10_000_000), // Rotate when the file exceeds 10 MB
            Naming::Numbers,             // Use numbers for rotated files
            Cleanup::KeepLogFiles(5),    // Keep at most 5 log files
        )
        .duplicate_to_stderr(Duplicate::Warn) // Duplicate logs to stderr
        .start()
        .unwrap();

    //Step 1
    println!("START");
    let start = Instant::now();
    altered_history::visit_timestamps::retrieve_visit_timestamps(&opts);
    if let Some(cp) = altered_history::load_checkpoint(&opts) {
        altered_history::main_all_database_mpsc_with_cp(&opts, cp);
    } else {
        altered_history::main_all_database_mpsc(&opts);
    }
    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    println!(
        "branch kept: {} | branch rejected: {}",
        env::BRANCHES_KEPT.load(Ordering::Relaxed),
        env::BRANCHES_REJECTED.load(Ordering::Relaxed)
    );
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    info!(
        "branch kept: {} | branch rejected: {}",
        env::BRANCHES_KEPT.load(Ordering::Relaxed),
        env::BRANCHES_REJECTED.load(Ordering::Relaxed)
    );

    //Focus
    let start = Instant::now();
    if altered_history::analysis::focus_missing_commits_all_files_with_save(&opts) {
        info!("Finished!");
    }
    println!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    info!("Focus complete | time elapsed: {:.2?}", start.elapsed());

    //Classes
    let start = Instant::now();
    classes::classification_all(&opts);
    println!(
        "Classification complete | time elapsed: {:.2?}",
        start.elapsed()
    );
    info!(
        "Classification complete | time elapsed: {:.2?}",
        start.elapsed()
    );
}
