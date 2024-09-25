use altered_history::classes;
use clap::Parser;
use std::time::Instant;
use log::LevelFilter;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use log::info;

fn main(){
    let opts = altered_history::env::Options::parse();

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, Config::default(), std::fs::File::create("./results/2021/new/2021-all.log").unwrap()),
        ]
    ).unwrap();

    //Main work
    let start = Instant::now();
    if let Some(cp) = altered_history::load_checkpoints(&opts){
        altered_history::main_all_database_mpsc_with_cp(&opts, cp);
    }
    else{
        altered_history::main_all_database_mpsc(&opts);
    }
    info!("Main work complete | time elapsed: {:.2?}", start.elapsed());

    //Focus
    let start = Instant::now();
    if altered_history::analysis::focus_missing_commits_all_files_with_save(&opts){
        info!("Finished!");
    }
    info!("Focus complete | time elapsed: {:.2?}", start.elapsed());
    
    //Classes
    let start = Instant::now();
    classes::classification_all(&opts);
    info!("Classification complete | time elapsed: {:.2?}", start.elapsed());
}
