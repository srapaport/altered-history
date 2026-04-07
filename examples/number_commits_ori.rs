use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use counter::Counter;
use rayon::prelude::*;

use indicatif::{ProgressBar, ProgressStyle};
use swh_graph::{graph::*, labels::EdgeLabel, mph::DynMphf, NodeType};
fn main(){

    let graph = SwhUnidirectionalGraph::new("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph")
    //let graph = SwhUnidirectionalGraph::new("/home/infres/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph")
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

    let origins = (0..graph.num_nodes())
        .filter(|&node| graph.properties().node_type(node) == NodeType::Origin)
        .collect::<Vec<usize>>();

    // let snapshot_nodes: Vec<usize> = (0..graph.num_nodes())
    //     .filter(|&node| graph.properties().node_type(node) == NodeType::Snapshot)
    //     .collect();

    let bar = Arc::new(ProgressBar::new(origins.len() as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("going through graph");
    
    origins.into_par_iter().for_each(|node| {  
        if let Some(msg) = graph.properties().message(node){
            if let Ok(url) = String::from_utf8(msg){
                let count = count_rev(node, &graph);
                let mut global_res = res.lock().unwrap();
                global_res[&url] += count;
            }
        }
    });
    bar.finish();
    
    let res = Arc::try_unwrap(res).unwrap().into_inner().unwrap();
    let mut csv_wrt = csv::WriterBuilder::new()
        .from_path(format!("results/commits_per_ori.csv"))
        .unwrap();
    csv_wrt.write_record(&["origin", "count"]).unwrap();
    for (origin, count) in res{
        csv_wrt.write_record(&[origin, format!("{}", count)]).unwrap();
    }
    csv_wrt.flush().unwrap();
}

fn count_rev<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    origin: usize,
    graph: &G)
-> usize
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut count = 0;
    let mut to_visit = vec![origin];
    let mut visited = HashSet::new();
    while let Some(visiting) = to_visit.pop(){
        if visited.contains(&visiting){
            continue;
        }
        visited.insert(visiting);
        for succ in graph.successors(visiting){
            match graph.properties().node_type(succ){
                NodeType::Revision => {
                    count += 1;
                    to_visit.push(succ);
                },
                NodeType::Release | NodeType::Snapshot =>{
                    to_visit.push(succ);
                }
                _ => {}
            }

        }
    }
    count
}
