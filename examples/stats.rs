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

static ORI_KEPT: AtomicUsize = AtomicUsize::new(0);
static ORI_REJECTED: AtomicUsize = AtomicUsize::new(0);
static BRANCHES_KEPT: AtomicUsize = AtomicUsize::new(0);
static BRANCHES_REJECTED: AtomicUsize = AtomicUsize::new(0);
static TOTAL_REV: AtomicUsize = AtomicUsize::new(0);

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


    let graph = SwhUnidirectionalGraph::new(PathBuf::from(basename))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    count(&graph);
    
    
}

fn count<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    graph: &G
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{

    let props = graph.properties();
    //Look for origins
    for node in 0..graph.num_nodes(){
        if props.node_type(node) != NodeType::Origin{
            continue;
        }
        //Look for snapshots
        let mut kept: bool = false;
        let mut branches_kept = 0;
        let mut branches_rejected = 0;
        let mut nb_rev = 0;
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
                        todo!("Amount of commits in the branch"); 
                        //nb_rev += count_revisions(succ_snap, graph);
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
            TOTAL_REV.fetch_add(nb_rev, Ordering::Relaxed);
        }
        else{
            ORI_REJECTED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn count_revisions<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    rev_rel: usize,
    graph: &G
) -> usize
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut nb_rev = 0;
    //if rel -> rev else keep rev
    //while succ type rev, nb_rev += 1
    nb_rev
}
