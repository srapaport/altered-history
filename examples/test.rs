use std::{collections::VecDeque, path::PathBuf, time::Instant};
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::LevelFilter;
use swh_graph::{graph::{SwhForwardGraph, SwhGraph, SwhGraphWithProperties, SwhUnidirectionalGraph}, mph::DynMphf, NodeType, SwhGraphProperties};

pub fn main(){

    Logger::with(LevelFilter::Info)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./logs")
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

    // todo!("Missing commits in swh:1:ori:2150853287a431dc908ef264b8a3922a5f3bcfa9 | with pid: 2829474");
    // couldn't find dir for rev swh:1:rev:fe358f4849c005e57328d57a998ab9411e6bbe90
    // let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
    //     .expect("Could not load graph")
    //     .init_properties()
    //     .load_properties(|properties| properties.load_maps::<DynMphf>())
    //     .expect("Could not load maps")
    //     .load_properties(|properties| properties.load_label_names())
    //     .expect("Could no load label names")
    //     .load_labels()
    //     .expect("Could not load labels")
    //     .load_properties(SwhGraphProperties::load_strings)
    //     .expect("Could not load strings");
    // println!("ori : {}",
    //     String::from_utf8_lossy(
    //     &graph.properties().message(
    //         graph.properties().node_id("swh:1:ori:2150853287a431dc908ef264b8a3922a5f3bcfa9").unwrap()
    //     ).unwrap())
    // );

    // let ori = graph.properties().node_id("swh:1:ori:2150853287a431dc908ef264b8a3922a5f3bcfa9").unwrap();
    // let sorted_snapshots_tmp =
    //     altered_history::retrieve_sorted_snapshots(ori, &graph)
    //         .unwrap();
    // let sorted_snapshots: Vec<String> =
    //     altered_history::retrieve_sorted_snapshots(ori, &graph)
    //         .unwrap()
    //         .into_iter()
    //         .map(|(snap, _)| graph.properties().swhid(snap).to_string())
    //         .collect();
    // println!("snapshots\n{:?}", sorted_snapshots_tmp);
    // let mut queue = VecDeque::from(sorted_snapshots);
    // // "swh:1:snp:49d6a25ba6541aaa6a25141ade01ad5ec28b76e5", "swh:1:snp:c03ad9b857c4dc9843775b5d113bc6e0b464c861", "swh:1:snp:6f12eb19cb480b9bbe4e377ac92b787b848ab59e", "swh:1:snp:bb1c13f4f048f5f60c35b268ad958daaf8dcb537"
    // println!("swhid1: {:?}", queue.pop_front().unwrap());
    // println!("swhid2: {:?}", queue.pop_front().unwrap());
    // find_first_ori();
    // retrieve_succs();
    long_retrieval();

}

fn find_first_ori(){
    let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
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

    for i in 0..graph.num_nodes(){
        if graph.properties().node_type(i) == NodeType::Origin{
            println!("Origin: {i} | {}", graph.properties().swhid(i));
            let tmp = graph.properties().message(i).unwrap();
            let url = String::from_utf8_lossy(&tmp);
            println!("ori : {:?}", url);
            break;
        }
    }
}
    
fn retrieve_succs(){

    let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
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
    for succ in graph.successors(graph.properties().node_id(rev_swhid).unwrap()){
        let succ_swhid = graph.properties().swhid(succ).to_string();
        println!("succ: {:?}", succ_swhid);
    }
}

fn long_retrieval(){
    let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
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
    
    let start = Instant::now();
    let ori = graph.properties().node_id("swh:1:ori:2150853287a431dc908ef264b8a3922a5f3bcfa9").unwrap();
    let sorted_snapshots: Vec<String> =
        altered_history::retrieve_sorted_snapshots(ori, &graph)
            .unwrap()
            .into_iter()
            .map(|(snap, _)| graph.properties().swhid(snap).to_string())
            .collect();
    println!("retrieve_snapshots | time elapsed: {:.2?}", start.elapsed());
    println!("snapshots\n{:?}", sorted_snapshots);

    let start = Instant::now();
    if let Some(_) = altered_history::altered_commits(
        &mut VecDeque::from(sorted_snapshots),
        &graph
    ) {
        println!(
            "Missing commits in {} time elapsed: {:.2?}",
            ori,
            start.elapsed()
        );

    } else {
        println!(
            "No missing commits in {} time elapsed: {:.2?}",
            ori,
            start.elapsed()
        );
    }
}