use altered_history::classes;
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::info;
use log::LevelFilter;
use std::time::Instant;

fn main() {
    //let opts = altered_history::env::Options::parse();
    let opts = altered_history::env::Options {
        // orc_dir: String::from("/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-orc/origin_visit_status"),
        // output_dir: String::from("./results/db_2021"),
        orc_dir: String::from(""),
        output_dir: String::from(""),
        //graph: String::from("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph"),
        graph: String::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"),
        // graph: String::from(
        //     "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph",
        // ),
        //results: String::from("/infres/ir800/rapaport/results/FULL_2024_08"),
        results: String::from("./results_2024"),
        //expected_origins: 226_726_529,
        // expected_origins: 2_181,
        expected_origins: 443,
        //chunk: 10_000,
        chunk: 10,
        removed_branch: false,
    };

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./results/logs")
                .basename("2021")
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

    // Step 1
    println!("START");
    let start = Instant::now();
    // Retrieve all snapshots per origin with their timestamps (only for old teaser datasets)
    //altered_history::visit_timestamps::retrieve_visit_timestamps(&opts);

    let cp = altered_history::load_checkpoint(&opts);
    // altered_history::main_all_rocksdb_mpsc_with_cp(&opts, cp);
    altered_history::main_all_mpsc_with_cp(&opts, cp);

    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    // println!(
    //     "branch kept: {} | branch rejected: {}",
    //     env::BRANCHES_KEPT.load(Ordering::Relaxed),
    //     env::BRANCHES_REJECTED.load(Ordering::Relaxed)
    // );
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    // info!(
    //     "branch kept: {} | branch rejected: {}",
    //     env::BRANCHES_KEPT.load(Ordering::Relaxed),
    //     env::BRANCHES_REJECTED.load(Ordering::Relaxed)
    // );

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
