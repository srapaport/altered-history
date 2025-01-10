use altered_history::classes;
use altered_history::env;
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::info;
use log::LevelFilter;
use std::sync::atomic::Ordering;
use std::time::Instant;

fn main() {
    //let opts = altered_history::env::Options::parse();
    let opts = altered_history::env::Options {
        graph: String::from("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph"),
        //graph: String::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"),
        // graph: String::from(
        //     "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph",
        // ),
        results: String::from("/infres/ir800/rapaport/results/FULL_2024_08_v2"),
        //results: String::from("./results_2024"),
        expected_origins: 310_334_314,
        //expected_origins: 2_181,
        //expected_origins: 443,
        //chunk: 10,
        chunk: 50_000,
        removed_branch: false,
    };

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./logs_2024")
                .basename("FULL_2024")
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

    println!("START");
    // Step 1
    /*let start = Instant::now();

    let cp = altered_history::load_checkpoint(&opts);
    altered_history::main_all_mpsc_with_cp(&opts, cp);

    println!(
        "Origins rejected: {}",
        env::ORI_REJECTED.load(Ordering::Relaxed)
    );
    println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());*/


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
    info!(
        "Rev without a dir: {}",
        env::REV_WITHOUT_DIR.load(Ordering::Relaxed)
    );
    println!(
        "Rev without a dir: {}",
        env::REV_WITHOUT_DIR.load(Ordering::Relaxed)
    );
}
