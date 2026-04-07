use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use rayon::prelude::*;

use indicatif::{ProgressBar, ProgressStyle};
use swh_graph::labels::VisitStatus;
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

    let bar = Arc::new(ProgressBar::new(graph.num_nodes() as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("going through graph");

    let revision_cache: Arc<Mutex<HashMap<String, HashSet<usize>>>> = Arc::new(Mutex::new(HashMap::new()));
    
    (0..graph.num_nodes()).into_par_iter().for_each(|node|{
        bar.inc(1);
        if graph.properties().node_type(node) != NodeType::Origin{
            return;
        }

        let local_cache = Arc::new(Mutex::new(HashMap::new()));
        
        for (succ, labels) in graph.labeled_successors(node){
            if graph.properties().node_type(succ) != NodeType::Snapshot{
                continue;
            }
            for label in labels{
                if let EdgeLabel::Visit(visit) = label{
                    if visit.status() == VisitStatus::Full{
                        count_rev(succ, local_cache.clone(), &graph);
                    }
                }
            }
        }

        let local_cache = Arc::try_unwrap(local_cache).unwrap().into_inner().unwrap();
        let mut cache_guard = revision_cache.lock().unwrap();
        cache_guard.extend(local_cache);
        
    });

    bar.finish();
    
    let res = Arc::try_unwrap(revision_cache).unwrap().into_inner().unwrap();
    let mut csv_wrt = csv::WriterBuilder::new()
        .from_path(format!("results/commits_per_branch_FULL.csv"))
        .unwrap();
    csv_wrt.write_record(&["branch_name", "count"]).unwrap();
    for (branch, revs) in res{
        csv_wrt.write_record(&[branch, format!("{}", revs.into_iter().count())]).unwrap();
    }
    csv_wrt.flush().unwrap();
}

fn count_rev<G: SwhLabeledForwardGraph + SwhGraphWithProperties + Sync>(
    snap: usize,
    cache: Arc<Mutex<HashMap<String, HashSet<usize>>>>,
    graph: &G)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut branch_revisions: HashMap<String, usize> = HashMap::new();

    for (succ, labels) in graph.labeled_successors(snap){
        if graph.properties().node_type(succ) == NodeType::Release{
            branch_revisions.extend(get_rel_labels(succ, graph));
        }
        else if graph.properties().node_type(succ) != NodeType::Revision{
            continue;
        }
        for label in labels{
            if let EdgeLabel::Branch(branch) = label{
                if let Ok(branch_name) = String::from_utf8(graph.properties().label_name(branch.filename_id())){
                    branch_revisions.insert(branch_name, succ);
                }
            }
        }
    }

    let mut revision_to_branches: HashMap<usize, Vec<String>> = HashMap::new();
    todo!("make the map global and use it as a cache -> is the rev has already been use for this branch ?");
    for (branch_name, rev) in branch_revisions {
        revision_to_branches.entry(rev).or_default().push(branch_name);
    }

    revision_to_branches
        .into_par_iter()
        .for_each(|(start_rev, branch_names)| {
            let reachable_revisions = get_reachable_revisions(start_rev, graph);
            let mut cache_guard = cache.lock().unwrap();
            branch_names.into_iter().for_each(|branch|{
                cache_guard.entry(branch).or_insert_with(HashSet::new).extend(reachable_revisions.clone());
            });
        });

}

fn get_rel_labels<G: SwhLabeledForwardGraph + SwhGraphWithProperties + Sync>(
    rel: usize,
    graph: &G
) -> HashMap<String, usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut branch_revisions = HashMap::new();
    for (succ, labels) in graph.labeled_successors(rel){
        if graph.properties().node_type(succ) != NodeType::Revision{
            continue;
        }
        for label in labels{
            if let EdgeLabel::Branch(branch) = label{
                if let Ok(branch_name) = String::from_utf8(graph.properties().label_name(branch.filename_id())){
                    branch_revisions.insert(branch_name, succ);
                }
            }
        }
    }
    branch_revisions
}

fn get_reachable_revisions<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    start_rev: usize,
    graph: &G,
) -> HashSet<usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
{
    
    let mut reachable = HashSet::new();
    let mut to_visit = vec![start_rev];
    let mut visited = HashSet::new();
    
    while let Some(visiting) = to_visit.pop() {
        if visited.contains(&visiting) {
            continue;
        }
        visited.insert(visiting);
        reachable.insert(visiting);
        
        for succ in graph.successors(visiting) {
            if graph.properties().node_type(succ) == NodeType::Revision{
                to_visit.push(succ);
            }
        }
    }

    reachable
}