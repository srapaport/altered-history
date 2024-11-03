use anyhow::{ensure, Result};
use altered_history::env;
use once_cell::sync::Lazy;
use swh_graph::graph::*;
use swh_graph::NodeType;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::ffi::OsString;
use std::io::{prelude::*, BufReader};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::sync::Mutex;
use std::cell::Cell;
use std::thread;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::path::PathBuf;
use ar_row::arrow::array::RecordBatch;
use chashmap::CHashMap;
use regex::Regex;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use rocksdb::{DB, Options};
use indicatif::{HumanCount, ProgressBar, ProgressStyle};
use swh_graph::graph::*;
use swh_graph::labels::EdgeLabel;
use swh_graph::java_compat::mph::gov::GOVMPH;
use log::{debug, info, warn};
use log::LevelFilter;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use clap::Parser;

extern "C" {
    fn gettid() -> u32;
}

pub static BRANCHES_STATUS_TEST: Lazy<Mutex<HashMap<String, (HashSet<String>, HashSet<String>)>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn main(){
    let opts = altered_history::env::Options::parse();

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Warn, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, Config::default(), std::fs::File::create("./examples/branches_filter_FULL.log").unwrap()),
        ]
    ).unwrap();

    let basename: &str;
    let db_path: String;
    let number_of_origins: usize; 
    let filename_res: &str;

    match opts.dataset.as_str(){
        "2021" => {
            basename = env::BASENAME_2021;
            db_path = "/infres/ir800/rapaport/phd-solal/dev/rust/datasets/2021".to_string();
            number_of_origins = env::ORIGINS_2021; 
            filename_res = "branches_filter_2021.csv";
        },
        "2023" => todo!(),
        "FULL" => {
            basename = env::BASENAME_FULL;
            db_path = format!("{}/db_origin_visit",env::DATABASE_FULL);
            number_of_origins = env::ORIGINS_FULL;
            filename_res = "branches_filter_FULL.csv";
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }



    let workers = num_cpus::get()/3;
    let mut options = Options::default();
    let db = DB::open(&options, db_path).unwrap();
    let count_origins = AtomicUsize::new(0);
    let pool = ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
    let bar = ProgressBar::new(number_of_origins as u64);
    bar.set_style(ProgressStyle::with_template("{wide_bar} {eta}").unwrap());
    let start = Instant::now();

    let graph = SwhUnidirectionalGraph::new(PathBuf::from(basename))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels"); 

    info!("Initialization over");

    let lock_file_w = Arc::new(Mutex::new(File::create(format!("./examples/{}", filename_res)).expect("Couldn't open File")));
    let mut file_w = lock_file_w.lock().unwrap();
    file_w
        .write("origin;branch;status\n".as_bytes())
        .expect(format!("couldn't write headings in file: {}", filename_res).as_str());
    std::mem::drop(file_w);

    pool.install(||{
        rayon::scope(|thread| {
            for result in db.iterator(rocksdb::IteratorMode::Start){
                match result{
                    Ok((key, value)) => {
                        thread.spawn(|_|{
                            let lock = lock_file_w.clone();
                            let key = key;
                            let value = value;
                            count_origins.fetch_add(1, Ordering::Relaxed);
                            let key_str = String::from_utf8(key.to_vec()).unwrap();
                            let thread_id = unsafe { gettid() };
                            info!("Checking Origin: {} | with pid: {:?}", key_str, thread_id);
        
                            let mut sorted_snapshots = altered_history::snapshots_sorting(value);
                            debug!("    sorted snapshots: {:?}", sorted_snapshots);

                            counting_branches(sorted_snapshots, key_str, lock, filename_res, &graph);

                            let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                            bar.set_position(curr_nb_origins);
                        });
                    },
                    Err(_) => continue,
                }
            }
        });
    });
    info!("Work Finished, time elapsed: {:.2?}", start.elapsed());

}

pub fn counting_branches<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snap_queue: VecDeque<String>,
    origin: String,
    file_w: Arc<Mutex<File>>, 
    filename_res: &str,
    graph: &G
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut non_snapshots = 0; 
    let mut keep = HashSet::new();
    let mut rejected = HashSet::new();    
    for snap_swhid in snap_queue{
        //Snapshot ID
        let swhid1 = format!("swh:1:snp:{snap_swhid}");
        let node_id = graph.properties().node_id(swhid1.as_str()).expect(format!("couldn't find swhid {} in full graph 2024-05", swhid1).as_str());
        if graph.properties().node_type(node_id) !=  NodeType::Snapshot{
            non_snapshots += 1;
            continue;
        }
        //All the successors of the snapshots
        let successors = graph.labeled_successors(node_id);
        for (_, labels) in successors {

            for label in labels{
                let branch_name: String;
                if let EdgeLabel::Branch(branch) = label{
                    branch_name = String::from_utf8_lossy(
                        &(graph.properties().label_name(branch.filename_id()))
                    ).to_string();
                    if !env::RE_BRANCH.is_match(&branch_name){
                        rejected.insert(branch_name.clone());
                        continue;
                    }
                    keep.insert(branch_name.clone());
                }
                else{
                    continue;
                }
            }
        }
    }
    let mut file_w = file_w.lock().unwrap();

    keep.iter().for_each(|branch|{
        file_w
            .write(format!("{};{};keep\n", origin, branch).as_bytes())
            .expect(format!("couldn't write datas in file: {}", filename_res).as_str());
    });
    rejected.iter().for_each(|branch|{
        file_w
            .write(format!("{};{};rejected\n", origin, branch).as_bytes())
            .expect(format!("couldn't write datas in file: {}", filename_res).as_str());
    });
    info!("amount of swhid non snapshots: {}", non_snapshots);
}