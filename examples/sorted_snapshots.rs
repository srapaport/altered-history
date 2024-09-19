use anyhow::{ensure, Result};
use swh_graph::graph::*;
use swh_graph::NodeType;
use swh_graph::labels::{EdgeLabel, VisitStatus};
use swh_graph::java_compat::mph::gov::GOVMPH;
use std::path::PathBuf;
use altered_history::env;
use log::{debug, info, warn};

fn main(){
    stderrlog::new()
        .verbosity(2)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    let graph = load_unidirectional(PathBuf::from(altered_history::env::BASENAME_2021))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    println!("hello example");
}

fn find_latest_snp<G>(graph: &G, ori: NodeId) -> Result<Option<(NodeId, u64)>>
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
    // Most recent snapshot thus far, as an optional (node_id, timestamp) pair
    let mut latest_snp: Option<(usize, u64)> = None;
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
                if let Some((_cur_snp, cur_ts)) = latest_snp {
                    if ts > cur_ts {
                        latest_snp = Some((succ, ts))
                    }
                } else {
                    latest_snp = Some((succ, ts))
                }
            }
        }
    }
    Ok(latest_snp)
}