use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use counter::Counter;
use rayon::prelude::*;

use indicatif::{ProgressBar, ProgressStyle};
use swh_graph::{graph::*, labels::EdgeLabel, mph::DynMphf, NodeType};
fn main(){

    //let graph = SwhUnidirectionalGraph::new("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph")
    let graph = SwhUnidirectionalGraph::new("/home/infres/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph")
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_properties(|properties| properties.load_strings())
        .expect("Could no load strings")
        .load_labels()
        .expect("Could not load labels");

    let res: Counter<String, usize> = Counter::new();
    let res = Arc::new(Mutex::new(res));

    let latest_snapshots: Vec<usize> = (0..graph.num_nodes())
        .filter(|&node| graph.properties().node_type(node) == NodeType::Origin)
        .filter_map(|ori| {
            altered_history::retrieve_sorted_snapshots(ori, &graph).ok().and_then(
               |snaps| {snaps.last().map(|&(snapshot_id, _)| snapshot_id)
            })
        }).collect();

    // let snapshot_nodes: Vec<usize> = (0..graph.num_nodes())
    //     .filter(|&node| graph.properties().node_type(node) == NodeType::Snapshot)
    //     .collect();

    let bar = Arc::new(ProgressBar::new(latest_snapshots.len() as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("going through graph");
    
    latest_snapshots.into_par_iter().for_each(|node| {
        let mut local_visited = HashSet::new();
        let mut local_res: Counter<String, usize> = Counter::new();
        
        for (succ, labels) in graph.labeled_successors(node){
            if graph.properties().node_type(succ) != NodeType::Revision{
                continue;
            }
            for label in labels{
                let branch_name: String;
                if let EdgeLabel::Branch(branch) = label {
                    branch_name =
                        String::from_utf8_lossy(&(graph.properties().label_name(branch.filename_id()))).to_string();
                } else {
                    continue;
                }
                local_res[&branch_name] += count_rev(branch_name.clone(), succ, &mut local_visited, &graph);
            }
        }
        
        // Merge local results into global counter
        let mut global_res = res.lock().unwrap();
        for (branch, count) in local_res {
            global_res[&branch] += count;
        }
        
        bar.inc(1);
    });
    bar.finish();
    
    let res = Arc::try_unwrap(res).unwrap().into_inner().unwrap();
    let mut csv_wrt = csv::WriterBuilder::new()
        .from_path(format!("results/commits_per_branch.csv"))
        .unwrap();
    csv_wrt.write_record(&["branch_name", "count"]).unwrap();
    for (branch, count) in res{
        csv_wrt.write_record(&[branch, format!("{}", count)]).unwrap();
    }
    csv_wrt.flush().unwrap();
}

fn count_rev<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    branch_name: String,
    rev: usize,
    visited: &mut HashSet<(String, usize)>,
    graph: &G)
-> usize
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut count = 0;
    let mut to_visit = vec![rev];
    while let Some(visiting) = to_visit.pop(){
        if visited.contains(&(branch_name.clone(), visiting)){
            continue;
        }
        visited.insert((branch_name.clone(), visiting));
        count += 1;
        for succ in graph.successors(visiting){
            if graph.properties().node_type(succ) == NodeType::Revision{
                if visited.contains(&(branch_name.clone(), succ)){
                    continue;
                }
                to_visit.push(succ);
            }
        }
    }
    count
}
