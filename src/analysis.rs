use crate::db;
use crate::env;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::info;
use rayon::prelude::*;
use sqlx::PgPool;
use std::collections::HashSet;
use std::path::PathBuf;
use swh_graph::graph::*;
use swh_graph::mph::DynMphf;

/// For a set of missing commits, extract the ones that don't have any parents in this set
/// return Set(origin, snapshot source, branch name, focused commit, snapshot dest)
fn focus_missing_commits<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties + Sync>(
    missing_commits: &'a HashSet<(String, String, String, String, String)>,
    graph: &G,
    progress: &MultiProgress,
) -> HashSet<(String, String, String, String, String)>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let rev_only: HashSet<String> = missing_commits
        .into_iter()
        .map(|commit| commit.3.clone())
        .collect();

    let bar = progress.add(ProgressBar::new(missing_commits.len() as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("Retrieving Root Cause Commits");
    let res = missing_commits
        .into_par_iter()
        .filter(|commit| {
            let node_id = graph.properties().node_id(commit.3.as_str()).unwrap();
            for succ in graph.successors(node_id) {
                let swhid = graph.properties().swhid(succ).to_string();
                if rev_only.contains(&swhid) {
                    bar.inc(1);
                    return false;
                }
            }
            bar.inc(1);
            true
        })
        .map(|commit| commit.clone())
        .collect();
    bar.finish_with_message("Done Retrieving");
    progress.remove(&bar);
    res
}

pub fn focus_missing_commits_from_db(
    opts: &env::Options,
    pool: &PgPool,
    rt: &tokio::runtime::Runtime,
    tables: &db::TableNames,
) -> bool {
    let graph_name: &str = opts.graph.as_str();
    let graph = SwhUnidirectionalGraph::new(PathBuf::from(graph_name))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_timestamps())
        .expect("Could not load timestamps")
        .load_properties(|properties| properties.load_persons())
        .expect("Could not load persons")
        .load_properties(|properties| properties.load_strings())
        .expect("Could not load strings")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    let multi_bar = MultiProgress::new();
    // Number of *whole origins* to load per batch. Bounds peak memory to one
    // batch worth of detected commits, mirroring the old per-file pipeline.
    let batch_origins = opts.chunk.max(1) as i64;
    let mut after = String::new();
    let mut total_detected: u64 = 0;
    let mut total_root_cause: u64 = 0;

    loop {
        let origins = rt
            .block_on(db::load_detected_origins_after(
                pool,
                tables,
                &after,
                batch_origins,
            ))
            .expect("Failed to load detected origins page");

        if origins.is_empty() {
            break;
        }
        after = origins.last().unwrap().clone();

        let detected = rt
            .block_on(db::load_detected_commits_for_origins(pool, tables, &origins))
            .expect("Failed to load detected commits for origins");
        total_detected += detected.len() as u64;

        let root_cause = focus_missing_commits(&detected, &graph, &multi_bar);
        total_root_cause += root_cause.len() as u64;

        rt.block_on(db::promote_root_cause_batch(pool, tables, &root_cause))
            .expect("Failed to promote root cause commits batch in DB");
    }

    rt.block_on(db::delete_remaining_detected(pool, tables))
        .expect("Failed to delete non-root-cause commits in DB");

    info!(
        "Focused to {} root cause commits (from {} detected)",
        total_root_cause, total_detected
    );

    true
}

pub fn focus_missing_commits_single_origin(
    opts: &env::Options,
    mc: &HashSet<(String, String, String, String, String)>,
) -> HashSet<(String, String, String, String, String)> {
    let graph_name: &str = opts.graph.as_str();

    let graph = SwhUnidirectionalGraph::new(PathBuf::from(graph_name))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_timestamps())
        .expect("Could not load timestamps")
        .load_properties(|properties| properties.load_persons())
        .expect("Could not load persons")
        .load_properties(|properties| properties.load_strings())
        .expect("Could not load strings")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    let focus = focus_missing_commits(mc, &graph, &MultiProgress::new());
    println!("------ Root Cause Commits ------");
    focus.iter().for_each(|ac| {
        println!("Snapshot with altered commit: {}", ac.1);
        println!("Branch name: {}", ac.2);
        println!("Altered commit: {}", ac.3);
        println!("Snapshot without altered commit: {}", ac.4);
        println!("---");
    });
    focus
}
