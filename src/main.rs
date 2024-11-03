use altered_history::classes;
use altered_history::env;
use clap::Parser;
use std::time::Instant;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use flexi_logger::{Logger, LogSpecBuilder, Criterion, Naming, Cleanup, Duplicate};
use std::sync::atomic::Ordering;
use log::LevelFilter;
use log::info;

fn main(){
    let opts = altered_history::env::Options::parse();

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
            .directory("/infres/ir800/rapaport/results/FULL_new/logs")
            .basename("FULL-all")
            .suffix("log")
        )
        .rotate(
            Criterion::Size(10_000_000), // Rotate when the file exceeds 10 MB
            Naming::Numbers,             // Use numbers for rotated files
            Cleanup::KeepLogFiles(5),    // Keep at most 5 log files
        )
        .duplicate_to_stderr(Duplicate::Warn) // Duplicate logs to stderr
        .start()
        .unwrap();

    // CombinedLogger::init(
    //     vec![
    //         TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
    //         WriteLogger::new(LevelFilter::Info, Config::default(), std::fs::File::create("/infres/ir800/rapaport/results/FULL_new/logs/FULL-all.log").unwrap()),
    //     ]
    // ).unwrap();

    //Main work 
    println!("START");
    // let start = Instant::now();
    // if let Some(cp) = altered_history::load_checkpoints(&opts){
    //     altered_history::main_all_database_mpsc_with_cp(&opts, cp);
    // }
    // else{
    //     altered_history::main_all_database_mpsc(&opts);
    // } 
    // println!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    // println!("branch kept: {} | branch rejected: {}", env::BRANCH_KEPT.load(Ordering::Relaxed), env::BRANCH_REJECTED.load(Ordering::Relaxed));
    // info!("Main work complete | time elapsed: {:.2?}", start.elapsed());
    // info!("branch kept: {} | branch rejected: {}", env::BRANCH_KEPT.load(Ordering::Relaxed), env::BRANCH_REJECTED.load(Ordering::Relaxed));

    //Focus
    // let start = Instant::now();
    // if altered_history::analysis::focus_missing_commits_all_files_with_save(&opts){
    //     info!("Finished!");
    // }
    // println!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    // info!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    
    //Classes
    let start = Instant::now();
    classes::classification_all(&opts);
    println!("Classification complete | time elapsed: {:.2?}", start.elapsed());
    info!("Classification complete | time elapsed: {:.2?}", start.elapsed());
}
