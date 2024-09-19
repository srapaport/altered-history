use altered_history::classes;
use clap::Parser;
use std::time::Instant;
use log::info;

fn main(){
    // Setup a stderr logger because ProgressLogger uses the `log` crate
    // to printout
    stderrlog::new()
        .verbosity(2)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    let opts = altered_history::env::Options::parse();
    /* let basename: String;
    match opts.dataset.as_str(){
        "2021" => basename = altered_history::env::BASENAME_2021.to_string(),
        "FULL" => basename = altered_history::env::BASENAME_FULL.to_string(),
        _ => unimplemented!(),
    } */
    
    /* let graph = load_unidirectional(PathBuf::from(&basename))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_timestamps())
        .expect("Could not load timestamps")
        .load_properties(|properties| properties.load_persons())
        .expect("Could not load persons")
        .load_properties(|properties| properties.load_strings())
        .expect("Could not load strings")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels"); */

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

    // sed -r -n 's/^.* branch: (.*) \|.*\|.*$/\1/p' res.log > branch_list.log
    // cat branch_list.log | sort -u > branch2.log
}
