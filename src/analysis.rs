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

/// Load all detected commits from DB, filter to root-cause, then update the DB.
/// Returns true when done.
pub fn focus_missing_commits_from_db(
    opts: &env::Options,
    pool: &PgPool,
    rt: &tokio::runtime::Runtime,
) -> bool {
    let detected = rt
        .block_on(db::load_detected_commits(pool))
        .expect("Failed to load detected commits from DB");

    if detected.is_empty() {
        info!("No detected commits to focus -- skipping.");
        return true;
    }

    info!("Loaded {} detected commits from DB", detected.len());

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
    let root_cause = focus_missing_commits(&detected, &graph, &multi_bar);

    info!(
        "Focused to {} root cause commits (from {} detected)",
        root_cause.len(),
        detected.len()
    );

    rt.block_on(db::promote_to_root_cause(pool, &root_cause))
        .expect("Failed to promote root cause commits in DB");

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
