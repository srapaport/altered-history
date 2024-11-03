use altered_history::env;
use swh_graph::graph::*;
use swh_graph::NodeType;
use std::collections::HashSet;
//use std::simd::num;
use std::sync::Arc;
use std::sync::Mutex;
use std::cell::Cell;
use std::thread;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::path::PathBuf;
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

static ORI_KEPT: AtomicUsize = AtomicUsize::new(0);
static ORI_REJECTED: AtomicUsize = AtomicUsize::new(0);
static BRANCHES_KEPT: AtomicUsize = AtomicUsize::new(0);
static BRANCHES_REJECTED: AtomicUsize = AtomicUsize::new(0);
static TOTAL_REV: AtomicUsize = AtomicUsize::new(0);

fn main(){
    let opts = altered_history::env::Options::parse();

    let stat_file: &str;

    match opts.dataset.as_str(){
        "2021" => {
            stat_file = "./examples/stats_2021.log";
        },
        "2023" => todo!(),
        "FULL" => {
            stat_file = "./examples/stats_FULL.log";
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Warn, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, Config::default(), std::fs::File::create(stat_file).unwrap()),
        ]
    ).unwrap();

    let start = Instant::now();
    count(&opts);
    info!("Stats complete | time elapsed: {:.2?}", start.elapsed());
    info!("Origins kept: {}", ORI_KEPT.load(Ordering::Relaxed));
    info!("Origins rejected: {}", ORI_REJECTED.load(Ordering::Relaxed));
    info!("Branches kept in origins kept: {}", BRANCHES_KEPT.load(Ordering::Relaxed));
    info!("Branches rejected in origins kept: {}", BRANCHES_REJECTED.load(Ordering::Relaxed));
    info!("Total revisions in branches kept: {}", TOTAL_REV.load(Ordering::Relaxed));
    
}

fn count(
    opts: &env::Options,
)
{
    let basename: &str;
    let number_of_origins: usize;

    match opts.dataset.as_str(){
        "2021" => {
            basename = env::BASENAME_2021;
            number_of_origins = env::ORIGINS_2021;
        },
        "2023" => todo!(),
        "FULL" => {
            basename = env::BASENAME_FULL;
            number_of_origins = env::ORIGINS_FULL;
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }
    let graph = SwhUnidirectionalGraph::new(PathBuf::from(basename))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    let props = graph.properties();
    let count_origins = AtomicUsize::new(0);
    let workers = num_cpus::get()/3;
    let pool = ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
    let bar = ProgressBar::new(number_of_origins as u64);
    bar.set_style(ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap());
    //Look for origins
    for node in 0..graph.num_nodes(){
        if props.node_type(node) != NodeType::Origin{
            continue;
        }
        pool.install(||{
            rayon::scope(|thread| {
                count_origins.fetch_add(1, Ordering::Relaxed);
                //Look for snapshots
                thread.spawn(|_|{
                    let mut kept: bool = false;
                    let mut branches_kept = 0;
                    let mut branches_rejected = 0;
                    let mut revs = HashSet::new();
                    let thread_id = unsafe { gettid() };
                    debug!("Checking Origin: {} | with pid: {:?}", node, thread_id);
                    for (succ_ori, _) in graph.labeled_successors(node){
                        if props.node_type(succ_ori) != NodeType::Snapshot{
                            warn!("succ of origin is not a snapshot");
                            continue;
                        }
                        //Look for branches
                        for (succ_snap, labels) in graph.labeled_successors(succ_ori){
                            if props.node_type(succ_snap) != NodeType::Revision && props.node_type(succ_snap) != NodeType::Release{
                                warn!("succ of snap is not a revision nor a release");
                                continue;
                            }
                            
                            for label in labels{
                                let branch_name: String;
                                if let EdgeLabel::Branch(branch) = label{
                                    branch_name = String::from_utf8_lossy(
                                        &(graph.properties().label_name(branch.filename_id()))
                                    ).to_string();
                                    if !env::RE_BRANCH.is_match(&branch_name){
                                        branches_rejected += 1;
                                        continue;
                                    }
                                    kept = true;
                                    branches_kept += 1;
                                    count_revisions(succ_snap, &mut revs, &graph);
                                }
                                else{
                                    continue;
                                }
                            }
                        }
                    }
                    if kept{
                        ORI_KEPT.fetch_add(1, Ordering::Relaxed);
                        BRANCHES_KEPT.fetch_add(branches_kept, Ordering::Relaxed);
                        BRANCHES_REJECTED.fetch_add(branches_rejected, Ordering::Relaxed);
                        TOTAL_REV.fetch_add(revs.into_iter().count(), Ordering::Relaxed);
                    }
                    else{
                        ORI_REJECTED.fetch_add(1, Ordering::Relaxed);
                    }
                    let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                    bar.set_position(curr_nb_origins);
                });
            });
        });
    }
    bar.finish_and_clear();
}

fn count_revisions<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    rev_rel: usize,
    revs: &mut HashSet<usize>,
    graph: &G
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let props = graph.properties();
    let mut rev_opt= None;
    if props.node_type(rev_rel) == NodeType::Release{
        for succ in graph.successors(rev_rel){
            if props.node_type(succ) != NodeType::Release{
                let x = props.node_type(succ).to_str();
                warn!("succ of release is not a release {x}");
                continue;
            }
            rev_opt = Some(succ);
        }
    }
    else{
        rev_opt = Some(rev_rel);
    }
    if let Some(rev) = rev_opt{
        let mut nodes_to_visit = Vec::new();
        nodes_to_visit.push(rev);
        let mut visited: HashSet<usize> = HashSet::new();
        while let Some(node) = nodes_to_visit.pop(){
            //debug!("size nodes_to_visit: {}", nodes_to_visit.len());
            if visited.contains(&node){
                continue;
            }
            visited.insert(node);
            let successors = graph.successors(node);
            for succ in successors{
                if (props.node_type(succ) != NodeType::Revision) && (props.node_type(succ) != NodeType::Release){
                    continue;
                }
                nodes_to_visit.push(succ);
                revs.insert(node);
            }
        }
    }
}
