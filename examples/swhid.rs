use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use altered_history::env;
use log::debug;
use log::info;
use log::LevelFilter;
use csv::Reader;
use swh_graph::graph::SwhLabeledBackwardGraph;
use std::error::Error;
use std::collections::HashSet;
use std::fmt::format;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::{collections::VecDeque, path::PathBuf, time::Instant};
use swh_graph::labels;
use swh_graph::labels::DirEntry;
use swh_graph::{
    graph::{
        SwhForwardGraph, SwhGraph, SwhGraphWithProperties, SwhLabeledForwardGraph,
        SwhUnidirectionalGraph,
        SwhBidirectionalGraph,
    },
    labels::EdgeLabel,
    mph::DynMphf,
    NodeType, SwhGraphProperties,
};

pub fn main() {
    Logger::with(LevelFilter::Debug)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./test_logs")
                .basename("swhid")
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
    //let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
    let graph = SwhBidirectionalGraph::new(PathBuf::from(
        "/poolswh/softwareheritage/graph/2024-08-23/compressed/graph",
    ))
    .expect("Could not load graph")
    .init_properties()
    .load_properties(|properties| properties.load_maps::<DynMphf>())
    .expect("Could not load maps")
    .load_properties(|properties| properties.load_label_names())
    .expect("Could no load label names")
    .load_labels()
    .expect("Could not load labels")
    .load_properties(SwhGraphProperties::load_strings)
    .expect("Could not load strings");

    let origins = rev_to_ori("swh:1:rev:630cb8507b2f1d7d7af3ac0f992d40f209dc1cee", &graph);
    for ori in origins{
        println!("ori: {:?}", ori);
    }
    //parser()
    //short_load();
    //example();
    //println!("{}",replace_semicolon("https://test;url;"));

    //let ori = 32285999;
    //let swhid = "swh:1:ori:64a5d4ddcabdc5089e2bbba7e0b4d3e8a1932521";
    //let ori = graph.properties().node_id(swhid).unwrap();
    //long_retrieval(swhid, &graph);

    // if NodeType::Origin == graph.properties().node_type(ori){
    //     println!("Yesss : {}", graph.properties().swhid(ori));
    //     let url = graph.properties().message(ori).unwrap();
    //     println!("url: {:?}", String::from_utf8_lossy(&url));
    // }
    // else {
    //     println!("Nooo");
    // }

    // let snap_swhid = "swh:1:snp:befb9dc49f9f70510fdc2dfc40e6e8e3c25ce87e";
    // let snap_id = graph.properties().node_id(snap_swhid).unwrap();
    // info!("snap_id: {snap_id}");
    // for (succ, labels) in graph.labeled_successors(snap_id){
    //     info!("succ: {:?}", graph.properties().swhid(succ).to_string());
    //     for label in labels{
    //         match label{
    //             EdgeLabel::Visit(v)=>{
    //                 info!("  Visit: timestamps: {} | status: {:?}", v.timestamp(), v.status());
    //             },
    //             EdgeLabel::Branch(b)=>{
    //                 info!(
    //                     "  Branch: {:?}",
    //                     String::from_utf8_lossy(&graph.properties().label_name(b.filename_id()))
    //                 );
    //             },
    //             EdgeLabel::DirEntry(d)=>{
    //                 info!("  DirEntry: {:?}", graph.properties().label_name(d.filename_id()));
    //             }
    //         }
    //     }
    // }
    // for succ in graph.successors(7774891076){
    //     println!("succ2: {succ}");
    // }

    // let a = altered_history::retrieve_sorted_snapshots(ori, &graph).unwrap();
    // let sorted_snapshots: Vec<String> =
    //     a
    //         .into_iter()
    //         .map(|(snap, _)| {
    //             graph.properties().swhid(snap).to_string()
    //         })
    //         .filter(|snap|{
    //             snap != "swh:1:snp:1a8893e6a86f444e8be8e7bda6cb34fb1735a00e"
    //         })
    //         .collect();
    // println!("      sorted snapshots:\n{:?}", sorted_snapshots);

    //let rel = 49329051;
    // for (succ, labels) in graph.labeled_successors(rel) {
    //     info!("succ: {}", graph.properties().swhid(succ).to_string());
    //     for label in labels {
    //         match label {
    //             EdgeLabel::Visit(v) => {
    //                 info!(
    //                     "  Visit: timestamps: {} | status: {:?}",
    //                     v.timestamp(),
    //                     v.status()
    //                 );
    //             }
    //             EdgeLabel::Branch(b) => {
    //                 info!(
    //                     "  Branch: {:?}",
    //                     String::from_utf8_lossy(&graph.properties().label_name(b.filename_id()))
    //                 );
    //             }
    //             EdgeLabel::DirEntry(d) => {
    //                 info!(
    //                     "  DirEntry: {:?}",
    //                     graph.properties().label_name(d.filename_id())
    //                 );
    //             }
    //         }
    //     }
    // }

    // let mut head = None;
    // let props = graph.properties();
    // let mut to_visit = VecDeque::new();
    // to_visit.push_back(rel);
    // let mut visited = HashSet::new();
    // 'visit: while let Some(node) = to_visit.pop_front() {
    //     info!("to visit: {:?}", to_visit);
    //     if visited.contains(&node){
    //         continue;
    //     }
    //     visited.insert(node);
    //     let successors = graph.successors(node);
    //     for succ in successors {
    //         info!("succ: {} {}", props.swhid(succ).to_string(), props.node_type(succ));
    //         match props.node_type(succ){
    //             NodeType::Revision=>{
    //                 head = Some(succ);
    //                 break 'visit;
    //             }
    //             NodeType::Release=>{
    //                 info!("RELEASE");
    //                 to_visit.push_back(succ);
    //             }
    //             _ => ()
    //         }
    //     }
    // }
    // info!("head : {:?}", head);
}

fn find_first_ori() {
    let graph = SwhUnidirectionalGraph::new(PathBuf::from(
        "/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph",
    ))
    .expect("Could not load graph")
    .init_properties()
    .load_properties(|properties| properties.load_maps::<DynMphf>())
    .expect("Could not load maps")
    .load_properties(|properties| properties.load_label_names())
    .expect("Could no load label names")
    .load_labels()
    .expect("Could not load labels")
    .load_properties(SwhGraphProperties::load_strings)
    .expect("Could not load strings");

    for i in 0..graph.num_nodes() {
        if graph.properties().node_type(i) == NodeType::Origin {
            println!("Origin: {i} | {}", graph.properties().swhid(i));
            let tmp = graph.properties().message(i).unwrap();
            let url = String::from_utf8_lossy(&tmp);
            println!("ori : {:?}", url);
            break;
        }
    }
}

fn retrieve_succs() {
    let graph = SwhUnidirectionalGraph::new(PathBuf::from(
        "/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph",
    ))
    .expect("Could not load graph")
    .init_properties()
    .load_properties(|properties| properties.load_maps::<DynMphf>())
    .expect("Could not load maps")
    .load_properties(|properties| properties.load_label_names())
    .expect("Could no load label names")
    .load_labels()
    .expect("Could not load labels")
    .load_properties(SwhGraphProperties::load_strings)
    .expect("Could not load strings");

    let rev_swhid = "swh:1:rev:fe358f4849c005e57328d57a998ab9411e6bbe90";
    for succ in graph.successors(graph.properties().node_id(rev_swhid).unwrap()) {
        let succ_swhid = graph.properties().swhid(succ).to_string();
        println!("succ: {:?}", succ_swhid);
    }
}

fn long_retrieval<G>(swhid: &str, graph: &G)
where
    G: SwhLabeledForwardGraph + SwhGraphWithProperties,
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    debug!("start debug");
    let start = Instant::now();
    let ori = graph.properties().node_id(swhid).unwrap();
    let sorted_snapshots: Vec<String> = altered_history::retrieve_sorted_snapshots(ori, &graph)
        .unwrap()
        .into_iter()
        .map(|(snap, _)| graph.properties().swhid(snap).to_string())
        .filter(|snap| snap != "swh:1:snp:1a8893e6a86f444e8be8e7bda6cb34fb1735a00e")
        .collect();
    info!("retrieve_snapshots | time elapsed: {:.2?}", start.elapsed());
    info!(" snapshots: {:?}", sorted_snapshots);

    let start = Instant::now();
    if let Some(_) =
        altered_history::altered_commits(&mut VecDeque::from(sorted_snapshots), false, graph)
    {
        info!(
            "Missing commits in {} time elapsed: {:.2?}",
            ori,
            start.elapsed()
        );
    } else {
        info!(
            "No missing commits in {} time elapsed: {:.2?}",
            ori,
            start.elapsed()
        );
    }
}

fn short_load(){
    let opts = altered_history::env::Options {
        graph: String::from("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph"),
        //graph: String::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"),
        // graph: String::from(
        //     "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph",
        // ),
        results: String::from("/infres/ir800/rapaport/results/FULL_2024_08"),
        //results: String::from("./results_2024"),
        expected_origins: 310_334_314,
        // expected_origins: 2_181,
        //expected_origins: 443,
        //chunk: 10_000,
        chunk: 100_000,
        removed_branch: false,
    };
    // 461_449_920
    // 916_856_162
    let start = Instant::now();
    let dir = fs::read_dir(opts.results.as_str())
        .expect(format!("can't read dir {}", opts.results).as_str());
    for file in dir{
        let filename = &file
            .unwrap()
            .path()
            .file_name()
            .unwrap()
            .to_os_string()
            .into_string()
            .unwrap();
        if filename == "swh:1:snp:1b0d48cd5749808cda6a5eb1dea0da22c7220869.csv"{
            continue;
        }
        if env::RE_CSV.is_match(&filename) {
            let _ = altered_history::analysis::load_file_results(&opts, &filename);
        }
        break;
    }
    println!("short_load | time elapsed: {:.2?}", start.elapsed());
}

fn example(){
    let data = "\
        city,country,pop
        Boston,United States,4628910
        ";
    let mut cpt = 0;
    let x = csv::ReaderBuilder::new()
        .delimiter(b',')
        .from_reader(data.as_bytes())
        .records()
        .into_iter()
        .for_each(|res|{
            cpt += 1;
            if let Ok(srecord) = res{
                println!("{:?}", srecord);
            }
        });
    println!("cpt: {}", cpt);
}

fn replace_semicolon(source: &str) -> String {
    return str::replace(source, ";", "&AlteredCommitSemicolon");
}
 
fn parser(){
    let start = Instant::now();
    let file = File::open("/infres/ir800/rapaport/results/FULL_2024_08/swh:1:snp:1b0d48cd5749808cda6a5eb1dea0da22c7220869.csv").unwrap();
    let reader = BufReader::new(file);

    let mut line_number = 0;
    for line in reader.lines(){
        line_number += 1;
        if let Ok(line) = line{
            info!("line: {:?}", line);
            let csv_line: Vec<&str> = line.split(';').collect();
            info!("csv line: {:?}", csv_line);
            if csv_line.len() > 5{
                break;
            }
        }
        else{
            info!("Couldn't read line {}", line_number);
        }
    }
    info!("line parsed: {} | time elapsed: {:.2?}", line_number, start.elapsed());
}

fn rev_to_ori<G>(
    rev: &str,
    graph: &G
) -> HashSet<String>
where
    G: SwhLabeledForwardGraph + SwhLabeledBackwardGraph + SwhGraphWithProperties,
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
{ 
    let start = Instant::now();
    let mut res = HashSet::new();
    let props = graph.properties();
    let rev_id = props.node_id(rev).unwrap();

    let mut to_visit = VecDeque::new();
    to_visit.push_back(rev_id);
    let mut visited = HashSet::new();
    while let Some(node) = to_visit.pop_back(){
        if visited.contains(&node){
            continue;
        }
        visited.insert(node);
        for pred in graph.predecessors(node){
            match props.node_type(pred){
                NodeType::Origin =>{ 
                    let url = props.message(node).unwrap();
                    res.insert(String::from_utf8_lossy(&url).to_string());
                }
                NodeType::Release | NodeType::Revision=>{
                    to_visit.push_front(node);
                }
                _ =>{
                    continue;
                }
            }
        }
    }
    println!("time elapsed: {:.2?}", start.elapsed());
    res
}