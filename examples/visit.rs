use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::LevelFilter;
use std::path::PathBuf;
use swh_graph::{
    graph::{SwhBidirectionalGraph, SwhGraph, SwhGraphWithProperties, SwhLabeledForwardGraph},
    labels::{EdgeLabel, VisitStatus},
    mph::DynMphf,
    NodeType, SwhGraphProperties,
};

fn main() {
    Logger::with(LevelFilter::Debug)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./test_logs")
                .basename("timestamp_error")
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

    for ori in 0..graph.num_nodes() {
        if graph.properties().node_type(ori) != NodeType::Origin {
            continue;
        }
        println!(
            "checking ori: {}",
            String::from_utf8(graph.properties().message(ori).unwrap()).unwrap()
        );
        let mut b = false;
        for (_, labels) in graph.labeled_successors(ori) {
            for label in labels {
                if let EdgeLabel::Visit(visit) = label {
                    println!("ts: {}", visit.timestamp());
                    println!(
                        "status: {}",
                        match visit.status() {
                            VisitStatus::Full => "Full",
                            VisitStatus::Partial => "Partial",
                        }
                    );
                    b = true;
                }
            }
        }
        if b {
            break;
        }
    }
}
