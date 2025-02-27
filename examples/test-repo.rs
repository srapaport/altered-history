use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::LevelFilter;
use std::{path::PathBuf, time::Instant};
use swh_graph::{
    graph::{SwhBidirectionalGraph, SwhGraphWithProperties, SwhLabeledForwardGraph},
    mph::DynMphf,
    SwhGraphProperties,
};


fn main(){
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
    let opts = altered_history::env::Options {
        graph: String::from("/poolswh/softwareheritage/graph/2024-08-23/compressed/graph"),
        //graph: String::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"),
        // graph: String::from(
        //     "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph",
        // ),
        results: String::from("/infres/ir800/rapaport/results/FULL_2024_08_v2"),
        //results: String::from("./results_2024"),
        expected_origins: 310_334_314,
        //expected_origins: 2_181,
        //expected_origins: 443,
        //chunk: 10,
        chunk: 50_000,
        removed_branch: false,
    };

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


    /* let start = Instant::now();

    let origin = "https://github.com/srapaport/missing_commits";
    let Some(mc) = altered_history::main_single_origin(&opts, origin) else{
        println!("No missing commit in {origin}");
        return;
    };
    let mc = mc.into_iter().map(|commit| (origin.to_string(), commit.0, commit.1, commit.2, commit.3)).collect();
    let roots = altered_history::analysis::focus_missing_commits_single_origin(&opts, &mc);
    let _classes = altered_history::classes::classification_single_origin(&opts, roots);

    println!("time elapsed: {:.2?}", start.elapsed()); */

    /* let swhid = "swh:1:rev:2b871f4dbffe3801d0da3f89806b5935f758d5f3";
    for (_, labels) in graph.labeled_successors(graph.properties().node_id(swhid).unwrap()){
        for label in labels{

        }
    } */

}