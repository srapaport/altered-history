use anyhow::{ensure, Result};
use swh_graph::graph::*;
use swh_graph::NodeType;
use swh_graph::labels::{EdgeLabel, VisitStatus};
use swh_graph::java_compat::mph::gov::GOVMPH;
use swh_graph::SwhGraphProperties;
use std::path::PathBuf;
use altered_history::env;
use sha1::{Sha1, Digest};
use log::{debug, info, warn};

fn main(){
    stderrlog::new()
        .verbosity(2)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    let graph = SwhBidirectionalGraph::new(PathBuf::from(altered_history::env::BASENAME_FULL_2024))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_properties(SwhGraphProperties::load_strings)
        .expect("Could not load messages")
        .load_labels()
        .expect("Could not load labels");

    let mut url_hash = Sha1::new();
    //hash.update(b"https://pypi.org/project/pymssql/");
    url_hash.update("https://github.com/facebookresearch/ParlAI");
    let node_origin = graph.properties().node_id(format!("swh:1:ori:{:x}", url_hash.finalize()).as_str()).unwrap();
    
    println!("node ori: {}", node_origin);

    let sorted = retrieve_sorted_snapshots(&graph, node_origin).unwrap();

    /* let first_three = &sorted[..3];
    let last_three = &sorted[sorted.len().saturating_sub(3)..]; */
    
    /* println!("First three elements: {:?}", first_three);
    println!("Last three elements: {:?}", last_three); */

    assert_eq!(
        &sorted[..3], 
        &vec![(35385463739, 1522957935), (35385463767, 1528140959), (35385463732, 1530431825)]
    );
    assert_eq!(
        &sorted[sorted.len().saturating_sub(3)..], 
        &vec![(35385463715, 1711006699), (35385463715, 1711049004), (35385463715, 1711836121)]
    );


    /* let rev = "swh:1:rev:6c802418e93a51cbb746b011439ab783dd136de8";
    let ori = find_ori_from_rev(&graph, rev).unwrap();
    println!("ori : {}", ori); */
}

fn find_ori_from_rev<G>(graph: &G, rev_swhid: &str) -> Option<String>
where
    G: SwhLabeledBackwardGraph + SwhGraphWithProperties,
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
{
    let props = graph.properties();
    //let node_type = props.node_type(ori);
    // Most recent snapshot thus far, as an optional (node_id, timestamp) pair
    for (succ, _) in graph.labeled_predecessors(props.node_id(rev_swhid).unwrap()) {
        let node_type = props.node_type(succ);
        if node_type == NodeType::Origin {
            return Some(String::from_utf8_lossy(&props.message(succ).unwrap()).to_string());
        }
        else{
            return find_ori_from_rev(graph, props.swhid(succ).to_string().as_str());
        }
    }
    None
}

fn retrieve_sorted_snapshots<G>(
    graph: &G,
    ori: NodeId
) -> Result<Vec<(NodeId, u64)>>
where
    G: SwhLabeledForwardGraph + SwhGraphWithProperties,
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
{
    let props = graph.properties();
    let node_type = props.node_type(ori);
    ensure!(
        node_type == NodeType::Origin,
        "Type of {ori} should be origin, but is {node_type} instead"
    );
    let mut snapshots: Vec<(NodeId, u64)> = vec![];
    for (succ, labels) in graph.labeled_successors(ori) {
        let node_type = props.node_type(succ);
        if node_type != NodeType::Snapshot {
            continue;
        }
        for label in labels {
            if let EdgeLabel::Visit(visit) = label {
                if visit.status() != VisitStatus::Full {
                    continue;
                }
                let ts = visit.timestamp();
                snapshots.push((succ, ts));
            }
        }
    } 
    snapshots.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(snapshots)
}
