use std::collections::{HashMap, HashSet};
use swh_graph::graph::*;

pub fn map_pp(map: &HashMap<String, HashSet<String>>){
    for (branch_name, commits) in map{
        println!("Branch name: {}", branch_name);
        println!("    Revisions: {:?}", commits);
    }
}

pub fn read_node_id<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
        swhid: &str,
        graph: &G
    ) -> usize
    where
        <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
        <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    graph.properties().node_id(swhid).unwrap()
}