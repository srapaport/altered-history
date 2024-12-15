use csv;
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use indicatif::{ProgressBar, ProgressStyle};
use log::warn;
use log::LevelFilter;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::path::PathBuf;
use swh_graph::{
    graph::{
        SwhBidirectionalGraph, SwhForwardGraph, SwhGraph, SwhGraphWithProperties,
    },
    mph::DynMphf,
    NodeType, SwhGraphProperties,
};

fn main() -> Result<(), Box<dyn Error>> {
    Logger::with(LevelFilter::Debug)
        .log_to_file(
            flexi_logger::FileSpec::default()
                // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
                // .basename("FULL-all")
                .directory("./test_logs")
                .basename("snapshots")
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

    //url_message();
    snapshots_per_origin()?;
    Ok(())
}

fn snapshots_per_origin()-> Result<(), Box<dyn Error>>{
    let graph = SwhBidirectionalGraph::new(PathBuf::from(
        //"/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph",
        "/infres/ir800/rapaport/datasets/2023-09-06/compressed/graph",
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

    let snapshots_origin: Arc<Mutex<HashMap<String, AtomicUsize>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let bar = ProgressBar::new((graph.num_nodes()) as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    let workers = (num_cpus::get() / 3) * 2;
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let jh = thread::spawn({
        let snapshots_origin = snapshots_origin.clone();
        move || {
            pool.install(|| {
                rayon::scope(|thread| {
                    for node in 0..graph.num_nodes() {
                        if graph.properties().node_type(node) != NodeType::Origin {
                            continue;
                        }
                        thread.spawn({
                            let snapshots_origin = snapshots_origin.clone();
                            let graph = &graph;
                            move |_| {
                                let url: String;
                                if let Some(msg) = graph.properties().message(node){
                                    url = String::from_utf8_lossy(&msg).to_string();
                                }else{
                                    url = node.to_string();
                                }

                                for succ in graph.successors(node) {
                                    if graph.properties().node_type(succ) != NodeType::Snapshot {
                                        warn!("succ of origin is not a snapshot: {}", node);
                                        continue;
                                    }
                                    let mut snapshots_origin = snapshots_origin.lock().unwrap();
                                    if let Some(value) = snapshots_origin.get(&url) {
                                        value.fetch_add(1, Ordering::Relaxed);
                                    } else {
                                        snapshots_origin.insert(url.clone(), AtomicUsize::new(1));
                                    }
                                    std::mem::drop(snapshots_origin);
                                }
                            }
                        });
                        bar.set_position(node as u64);
                    }
                });
            });
            bar.finish_and_clear();
        }
    });
    jh.join().unwrap();

    let snapshots_origin = snapshots_origin.lock().unwrap();
    let mut csv_wrt = csv::Writer::from_path("./examples/snapshots_FULL.csv")?;
    csv_wrt.write_record(&["origin", "amount of snapshots"])?;
    snapshots_origin.iter().for_each(|ori| {
        csv_wrt
            .write_record(&[ori.0, &ori.1.load(Ordering::Relaxed).to_string()])
            .unwrap();
    });
    csv_wrt.flush()?;
    Ok(())
}

fn url_message(){
    let graph = SwhBidirectionalGraph::new(PathBuf::from(
        //"/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph",
        "/infres/ir800/rapaport/datasets/2023-09-06/compressed/graph",
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

    let node_ori = 1566528509;
    if graph.properties().node_type(node_ori) != NodeType::Origin {
        warn!("Not an origin");
        return;
    }
    else {
        warn!("origin check");
    }
    for succ in graph.successors(node_ori){
        if graph.properties().node_type(succ) == NodeType::Snapshot{
            warn!("Snapshot: {}", graph.properties().swhid(succ).to_string());
        }
    }
    let _ = graph.properties().message(node_ori).expect("couldn't");

}
