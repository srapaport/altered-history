pub mod analysis;
pub mod classes;
pub mod env;
pub mod visit_timestamps;
use anyhow::{ensure, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use rayon::ThreadPoolBuilder;
use rocksdb::{Options, DB};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use swh_graph::graph::*;
use swh_graph::java_compat::mph::gov::GOVMPH;
use swh_graph::labels::{EdgeLabel, VisitStatus};
use swh_graph::NodeType;

extern "C" {
    fn gettid() -> u32;
}

/// Returns a queue of snapshots in an ascendant chronological order
pub fn snapshots_sorting(snapshots: Box<[u8]>) -> VecDeque<String> {
    let mut sorted_snapshots = Vec::new();
    let decoded_value: env::Visits = bincode::deserialize(&snapshots[..]).unwrap();
    for (snapshot, ts) in decoded_value.snapshots.iter() {
        debug!("    snapshot id: {}", snapshot);
        debug!("        timestamps: {:?}", ts);
        if snapshot == env::EMPTY_SNAPSHOT {
            continue;
        }
        sorted_snapshots.push((snapshot.clone(), *ts.iter().min().unwrap()));
    }
    sorted_snapshots.sort_by(|a, b| a.1.cmp(&b.1));
    debug!("tuple sorted snaphosts: {:?}", sorted_snapshots);
    sorted_snapshots.into_iter().map(|(snap, _)| snap).collect()
}

/// Take a list of sorted snapshots and a graph
/// returns a set of all altered commits
/// -> (snapshot with altered commit, altered commit, branch name, snapshot without altered commit)
fn altered_commits<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snapshots: &mut VecDeque<String>,
    graph: &G,
) -> Option<HashSet<(String, String, String, String)>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut curr_altered_commits: HashSet<(String, String, String, String)> = HashSet::new();

    // We want to check if a commit is altered from a snapshot to another so we need at least 2 commits
    if snapshots.len() < 2 {
        return None;
    }
    let mut swhid1 = "swh:1:snp:".to_string() + &(snapshots.pop_front().unwrap());
    let mut set_branch_kept = HashSet::new();
    let mut set_branch_rejected = HashSet::new();
    let mut set_revs_analyzed = HashSet::new();
    let mut kept: bool = false;
    let mut swhid1_commits = find_all_commits(
        &swhid1,
        &mut set_branch_kept,
        &mut set_branch_rejected,
        &mut set_revs_analyzed,
        &mut kept,
        graph,
    )
    .unwrap();

    'find_altered_commits: loop {
        let swhid2 = "swh:1:snp:".to_string() + &(snapshots.pop_front().unwrap());
        let swhid2_commits = find_all_commits(
            &swhid2,
            &mut set_branch_kept,
            &mut set_branch_rejected,
            &mut set_revs_analyzed,
            &mut kept,
            graph,
        )
        .unwrap();

        let branch_altered_commits = compare_maps(&swhid1_commits, &swhid2_commits);
        if branch_altered_commits.len() > 0 {
            curr_altered_commits.extend(
                branch_altered_commits
                    .into_iter()
                    .map(
                        |(branch_name, commit)| -> (String, String, String, String) {
                            (swhid1.clone(), branch_name, commit, swhid2.clone())
                        },
                    )
                    .collect::<HashSet<(String, String, String, String)>>(),
            );
        }
        swhid1 = swhid2;
        swhid1_commits = swhid2_commits;

        if snapshots.len() < 1 {
            break 'find_altered_commits;
        }
    }
    env::BRANCHES_KEPT.fetch_add(set_branch_kept.len(), Ordering::Relaxed);
    env::BRANCHES_REJECTED.fetch_add(set_branch_rejected.len(), Ordering::Relaxed);
    debug!("Current missing commits: {:?}", curr_altered_commits);
    if kept {
        env::ORI_KEPT.fetch_add(1, Ordering::Relaxed);
        env::TOTAL_REV_ANALYZED.fetch_add(set_revs_analyzed.into_iter().count(), Ordering::Relaxed);
    } else {
        env::ORI_REJECTED.fetch_add(1, Ordering::Relaxed);
    }
    if curr_altered_commits.len() == 0 {
        return None;
    }
    Some(curr_altered_commits)
}

/// Takes a snapshot and returns a map
/// The map takes the branch name as an entry and has a set of all the commits in the branch as a value
/// The function also keeps track of the branch that are kept and rejected
fn find_all_commits<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snap_swhid: &String,
    set_branch_kept: &mut HashSet<String>,
    set_branch_rejected: &mut HashSet<String>,
    set_revs_analyzed: &mut HashSet<usize>,
    kept: &mut bool,
    graph: &G,
) -> Option<HashMap<String, HashSet<String>>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut all_commits: HashMap<String, HashSet<String>> = HashMap::new();
    //Check that the swhid given is a snapshot
    if graph
        .properties()
        .node_type(graph.properties().node_id(snap_swhid.as_str()).unwrap())
        != NodeType::Snapshot
    {
        return None;
    }
    //Snapshot ID
    let node_id = graph.properties().node_id(snap_swhid.as_str()).unwrap();
    //All the successors of the snapshots
    let successors = graph.labeled_successors(node_id);
    for (succ, labels) in successors {
        //succ is the ID a snapshot successor and labels are all the branches it is in
        let succ_swhid = graph.properties().swhid(succ).to_string();
        let succ_type = graph.properties().node_type(succ);
        //If the successor is not a revision or a release we don't wanna check its children nor add it to the list
        if !(succ_type == NodeType::Revision) && !(succ_type == NodeType::Release) {
            debug!("A snapshot successor is neither a release nor a revision. Snapshot: {} | Successor: {}", snap_swhid, succ_swhid);
            continue;
        }

        //We gather all the branches associated to this successor
        let mut branch_list: Vec<String> = Vec::new();
        for label in labels {
            let branch_name: String;
            //let filename = graph.properties().label_name(label);
            if let EdgeLabel::Branch(branch) = label {
                branch_name =
                    String::from_utf8_lossy(&(graph.properties().label_name(branch.filename_id())))
                        .to_string();
                if !env::RE_BRANCH.is_match(&branch_name) {
                    set_branch_rejected.insert(branch_name);
                    continue;
                }
                *kept = true;
                set_branch_kept.insert(branch_name.clone());
            } else {
                continue;
            }
            branch_list.push(branch_name);
        }

        if branch_list.len() == 0 {
            continue;
        }
        find_all_commits_aux(
            &mut all_commits,
            &branch_list,
            succ,
            set_revs_analyzed,
            graph,
        );
    }
    Some(all_commits)
}

/// This is an auxiliary function to find_all_commits
/// this function retrieves all commits that are ancestors of the given one
/// it will add them to the map associated to all the branches where the given commit is root
fn find_all_commits_aux<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    all_commits: &mut HashMap<String, HashSet<String>>,
    branch_list: &Vec<String>,
    node: usize,
    set_revs_analyzed: &mut HashSet<usize>,
    graph: &G,
) where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut nodes_to_visit = Vec::new();
    nodes_to_visit.push(node);
    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(node) = nodes_to_visit.pop() {
        //debug!("size nodes_to_visit: {}", nodes_to_visit.len());
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        let successors = graph.successors(node);
        for succ in successors {
            let succ_type = graph.properties().node_type(succ);
            if succ_type != NodeType::Revision && succ_type != NodeType::Release {
                continue;
            }
            let succ_swhid = graph.properties().swhid(succ).to_string();
            for branch in branch_list {
                all_commits
                    .entry(branch.clone())
                    .or_insert_with(HashSet::new)
                    .insert(succ_swhid.clone());
            }
            set_revs_analyzed.insert(succ);
            nodes_to_visit.push(succ);
        }
    }
}

/// retrieve all commits included in the first snapshot but not in the second
fn compare_maps(
    is_included: &HashMap<String, HashSet<String>>,
    include: &HashMap<String, HashSet<String>>,
) -> HashSet<(String, String)> {
    let mut not_included: HashSet<(String, String)> = HashSet::new();
    for (branch_name, commits_included) in is_included {
        match include.get(branch_name) {
            Some(include_commits) => {
                for commit_included in commits_included {
                    if include_commits.contains(commit_included) {
                        continue;
                    }
                    not_included.insert((branch_name.clone(), commit_included.clone()));
                }
            }
            None => {
                debug!("Branch: {}, couldn't be found", branch_name);
                //todo!("Add all the commits in the 'Removed Branch' class");
                continue;
            }
        }
    }
    debug!("Not included: {:?}", not_included);
    not_included
}

/// Find all altered commits from the given graph and orc files
/// Start at the beginning and create a checkpoint
/// Write the results at opts.results
pub fn main_all_database_mpsc(opts: &env::Options) {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let prefix_res: &str = opts.results.as_str();
    let number_of_origins: usize = opts.expected_origins;
    let chunk: usize = opts.chunk;
    let db_path = PathBuf::from(opts.output_dir.as_str());

    let (tx, rx) = channel::<(String, Option<HashSet<(String, String, String, String)>>)>();

    let jh = thread::spawn(move || {
        let workers = num_cpus::get() / 3;

        let graph = SwhUnidirectionalGraph::new(graph_path)
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");

        let options = Options::default();
        let db = DB::open(&options, db_path).unwrap();
        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
            )
            .unwrap(),
        );

        pool.install(|| {
            rayon::scope(|thread| {
                for result in db.iterator(rocksdb::IteratorMode::Start) {
                    match result {
                        Ok((key, value)) => {
                            thread.spawn({
                                let tx = tx.clone();
                                |_| {
                                    let tx = tx;
                                    let key = key;
                                    let value = value;
                                    count_origins.fetch_add(1, Ordering::Relaxed);
                                    let key_str = String::from_utf8(key.to_vec()).unwrap();
                                    let thread_id = unsafe { gettid() };
                                    info!(
                                        "Checking Origin: {} | with pid: {:?}",
                                        key_str, thread_id
                                    );

                                    let mut sorted_snapshots = snapshots_sorting(value);
                                    debug!("    sorted snapshots: {:?}", sorted_snapshots);

                                    ///////////// altered_commits
                                    if let Some(curr_altered_commits) =
                                        altered_commits(&mut sorted_snapshots, &graph)
                                    {
                                        info!("Missing commits in {}", key_str);
                                        tx.send((key_str, Some(curr_altered_commits)))
                                            .expect("Failed sending the msg");
                                    } else {
                                        tx.send((key_str, None)).expect("Failed sending the msg");
                                    }
                                    let curr_nb_origins =
                                        count_origins.load(Ordering::Relaxed) as u64;
                                    bar.set_position(curr_nb_origins);
                                }
                            });
                        }
                        Err(_) => continue,
                    }
                }
            });
        });
        bar.finish_and_clear();
    });

    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    File::create(format!("{}/checkpoints", prefix_res)).expect("couldn't create checkpoint file");

    // Receive results in parallel and write them at the frequency of opts.chunk
    for _ in 0..number_of_origins {
        let (ori, value) = rx.recv().expect("Couldn't receive data");
        if let Some(mc) = value {
            if res.len() > chunk {
                flush_map(prefix_res, &mut res);
                create_cp(prefix_res, &mut cp);
            }
            res.insert(ori.clone(), mc);
        }
        cp.insert(ori);
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0 {
        create_cp(prefix_res, &mut cp);
    }
    jh.join().unwrap();
}

/// Find all altered commits from the given graph and orc files
/// Start where the last checkpoint stoped
/// Write the results at opts.results
pub fn main_all_database_mpsc_with_cp(opts: &env::Options, checkpoint: HashSet<String>) {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let prefix_res: &str = opts.results.as_str();
    let number_of_origins: usize = opts.expected_origins;
    let chunk: usize = opts.chunk;
    let db_path = PathBuf::from(opts.output_dir.as_str());

    let (tx, rx) = channel::<Option<(String, Option<HashSet<(String, String, String, String)>>)>>();

    let jh = thread::spawn(move || {
        let workers = num_cpus::get() / 3;
        let graph = SwhUnidirectionalGraph::new(graph_path)
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");

        let options = Options::default();
        let db = DB::open(&options, db_path).unwrap();
        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
            )
            .unwrap(),
        );

        pool.install(|| {
            rayon::scope(|thread| {
                for result in db.iterator(rocksdb::IteratorMode::Start) {
                    match result {
                        Ok((key, value)) => {
                            thread.spawn({
                                let tx = tx.clone();
                                |_| {
                                    let tx = tx;
                                    let key = key;
                                    let value = value;
                                    count_origins.fetch_add(1, Ordering::Relaxed);
                                    let key_str = String::from_utf8(key.to_vec()).unwrap();
                                    ///////// Skipping if necessary according to the checkpoints
                                    if !checkpoint.contains(&key_str) {
                                        let thread_id = unsafe { gettid() };
                                        info!(
                                            "Checking Origin: {} | with pid: {:?}",
                                            key_str, thread_id
                                        );
                                        let mut sorted_snapshots = snapshots_sorting(value);
                                        debug!("    sorted snapshots: {:?}", sorted_snapshots);

                                        ///////////// altered_commits
                                        if let Some(curr_altered_commits) =
                                            altered_commits(&mut sorted_snapshots, &graph)
                                        {
                                            info!(
                                                "Missing commits in {} | with pid: {}",
                                                key_str,
                                                std::process::id()
                                            );
                                            tx.send(Some((key_str, Some(curr_altered_commits))))
                                                .expect("Failed sending the msg");
                                        } else {
                                            tx.send(Some((key_str, None)))
                                                .expect("Failed sending the msg");
                                        }
                                        let curr_nb_origins =
                                            count_origins.load(Ordering::Relaxed) as u64;
                                        bar.set_position(curr_nb_origins);
                                    } else {
                                        info!("Not calculating Origin: {}", key_str);
                                        tx.send(None).expect("Failed sending the msg");
                                    }
                                }
                            });
                        }
                        Err(_) => continue,
                    }
                }
            });
        });
        bar.finish_and_clear();
    });
    // Receive results in parallel and write them at the frequency of opts.chunk
    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    for _ in 0..number_of_origins {
        if let Some(package) = rx.recv().expect("Couldn't receive data") {
            let (ori, value) = package;
            if let Some(mc) = value {
                if res.len() > chunk {
                    flush_map(prefix_res, &mut res);
                    create_cp(prefix_res, &mut cp);
                }
                res.insert(ori.clone(), mc);
            }
            cp.insert(ori);
        }
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0 {
        create_cp(prefix_res, &mut cp);
    }
    jh.join().unwrap();
}

/// retrieves all snapshots of a given origin
/// and returns it in an chronologically ascendant order
pub fn retrieve_sorted_snapshots<G>(ori: NodeId, graph: &G) -> Result<Vec<(NodeId, u64)>>
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

/// Find all altered commits from the given graph and orc files
/// Start at the beginning and create a checkpoint
/// Write the results at opts.results
pub fn main_all_mpsc(opts: &env::Options) {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let prefix_res: &str = opts.results.as_str();
    let number_of_origins: usize = opts.expected_origins;
    let chunk: usize = opts.chunk;

    let (tx, rx) = channel::<(String, Option<HashSet<(String, String, String, String)>>)>();

    let jh = thread::spawn(move || {
        let workers = num_cpus::get() / 3;

        let graph = SwhUnidirectionalGraph::new(graph_path)
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");

        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
            )
            .unwrap(),
        );

        pool.install(|| {
            rayon::scope(|thread| {
                for ori in 0..graph.num_nodes() {
                    if graph.properties().node_type(ori) != NodeType::Origin {
                        continue;
                    }
                    thread.spawn({
                        let tx = tx.clone();
                        let count_origins = &count_origins;
                        let bar = &bar;
                        let graph = &graph;
                        move |_| {
                            let tx = tx;
                            count_origins.fetch_add(1, Ordering::Relaxed);
                            let ori_str = graph.properties().swhid(ori).to_string();
                            let thread_id = unsafe { gettid() };
                            info!("Checking Origin: {} | with pid: {:?}", ori_str, thread_id);
                            let sorted_snapshots: Vec<String> =
                                retrieve_sorted_snapshots(ori, &graph)
                                    .unwrap()
                                    .into_iter()
                                    .map(|(snap, _)| graph.properties().swhid(snap).to_string())
                                    .collect();
                            debug!("    sorted snapshots: {:?}", sorted_snapshots);

                            ///////////// altered_commits
                            if let Some(curr_altered_commits) =
                                altered_commits(&mut VecDeque::from(sorted_snapshots), &graph)
                            {
                                info!("Missing commits in {}", ori_str);
                                tx.send((ori_str, Some(curr_altered_commits)))
                                    .expect("Failed sending the msg");
                            } else {
                                tx.send((ori_str, None)).expect("Failed sending the msg");
                            }
                            let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                            bar.set_position(curr_nb_origins);
                        }
                    });
                }
            });
        });
        bar.finish_and_clear();
    });

    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    File::create(format!("{}/checkpoints", prefix_res)).expect("couldn't create checkpoint file");

    // Receive results in parallel and write them at the frequency of opts.chunk
    for _ in 0..number_of_origins {
        let (ori, value) = rx.recv().expect("Couldn't receive data");
        if let Some(mc) = value {
            if res.len() > chunk {
                flush_map(prefix_res, &mut res);
                create_cp(prefix_res, &mut cp);
            }
            res.insert(ori.clone(), mc);
        }
        cp.insert(ori);
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0 {
        create_cp(prefix_res, &mut cp);
    }
    jh.join().unwrap();
}

/// Find all altered commits from the given graph and orc files
/// Start where the last checkpoint stoped
/// Write the results at opts.results
pub fn main_all_mpsc_with_cp(opts: &env::Options, checkpoint: HashSet<String>) {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let prefix_res: &str = opts.results.as_str();
    let number_of_origins: usize = opts.expected_origins;
    let chunk: usize = opts.chunk;

    let (tx, rx) = channel::<Option<(String, Option<HashSet<(String, String, String, String)>>)>>();

    let jh = thread::spawn(move || {
        let workers = num_cpus::get() / 3;
        let graph = SwhUnidirectionalGraph::new(graph_path)
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");

        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
            )
            .unwrap(),
        );

        pool.install(|| {
            rayon::scope(|thread| {
                for ori in 0..graph.num_nodes() {
                    if graph.properties().node_type(ori) != NodeType::Origin {
                        continue;
                    }
                    thread.spawn({
                        let tx = tx.clone();
                        let count_origins = &count_origins;
                        let bar = &bar;
                        let graph = &graph;
                        let checkpoint = &checkpoint;
                        move |_| {
                            let tx = tx;
                            count_origins.fetch_add(1, Ordering::Relaxed);
                            let ori_str = graph.properties().swhid(ori).to_string();
                            ///////// Skipping if necessary according to the checkpoints
                            if !(*checkpoint).contains(&ori_str) {
                                let thread_id = unsafe { gettid() };
                                info!("Checking Origin: {} | with pid: {:?}", ori_str, thread_id);
                                let sorted_snapshots: Vec<String> =
                                    retrieve_sorted_snapshots(ori, &graph)
                                        .unwrap()
                                        .into_iter()
                                        .map(|(snap, _)| graph.properties().swhid(snap).to_string())
                                        .collect();
                                debug!("    sorted snapshots: {:?}", sorted_snapshots);

                                ///////////// altered_commits
                                if let Some(curr_altered_commits) =
                                    altered_commits(&mut VecDeque::from(sorted_snapshots), &graph)
                                {
                                    info!(
                                        "Missing commits in {} | with pid: {}",
                                        ori_str,
                                        std::process::id()
                                    );
                                    tx.send(Some((ori_str, Some(curr_altered_commits))))
                                        .expect("Failed sending the msg");
                                } else {
                                    tx.send(Some((ori_str, None)))
                                        .expect("Failed sending the msg");
                                }
                                let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                                bar.set_position(curr_nb_origins);
                            } else {
                                info!("Not calculating Origin: {}", ori_str);
                                tx.send(None).expect("Failed sending the msg");
                            }
                        }
                    });
                }
            });
        });
        bar.finish_and_clear();
    });
    // Receive results in parallel and write them at the frequency of opts.chunk
    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    for _ in 0..number_of_origins {
        if let Some(package) = rx.recv().expect("Couldn't receive data") {
            let (ori, value) = package;
            if let Some(mc) = value {
                if res.len() > chunk {
                    flush_map(prefix_res, &mut res);
                    create_cp(prefix_res, &mut cp);
                }
                res.insert(ori.clone(), mc);
            }
            cp.insert(ori);
        }
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0 {
        create_cp(prefix_res, &mut cp);
    }
    jh.join().unwrap();
}

/// Find all altered commits from a given origin
/// returns a set of (snapshot with altered commit, altered commit, branch name, snapshot without altered commit)
pub fn main_targeted_origin(
    opts: &env::Options,
    ori: &str,
) -> Option<HashSet<(String, String, String, String)>> {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let db_path = PathBuf::from(opts.output_dir.as_str());

    let graph = SwhUnidirectionalGraph::new(graph_path)
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels");

    let options = Options::default();
    let db = DB::open(&options, db_path).unwrap();

    let value: Box<[u8]> = db.get(ori).unwrap().unwrap().into();

    info!("Origin: {}", ori);

    let mut sorted_snapshots = snapshots_sorting(value);
    debug!("    sorted snapshots: {:?}", sorted_snapshots);

    ///////////// altered_commits
    let Some(curr_altered_commits) = altered_commits(&mut sorted_snapshots, &graph) else {
        info!("No missing commit in {}", ori);
        return None;
    };
    Some(curr_altered_commits)
}

/// writes the current results with chunk amount of results in prefix
fn flush_map(prefix: &str, map: &mut HashMap<String, HashSet<(String, String, String, String)>>) {
    let swhid = format!("{}", map.iter().next().unwrap().1.iter().next().unwrap().0);
    let filename = format!("{}/{}.csv", prefix, swhid);
    let filename_tmp = format!("{}/{}.tmp", prefix, swhid);
    let Ok(mut file_w) = File::create_new(format!("{}", filename_tmp)) else {
        warn!("File {} already exists!", filename_tmp);
        return;
    };
    file_w
        .write("origin;snapshot_src;branch_name;missing_commit;snapshot_dst\n".as_bytes())
        .expect(format!("couldn't write headings in file: {}", filename_tmp).as_str());
    map.iter().for_each(|v| {
        v.1.iter().for_each(|set| {
            file_w
                .write(format!("{};{};{};{};{}\n", v.0, set.0, set.1, set.2, set.3).as_bytes())
                .expect(format!("couldn't write datas in file: {}", filename_tmp).as_str());
        });
    });
    map.clear();
    drop(file_w); // didn't end with this instruction one time
    std::fs::rename(&filename_tmp, &filename)
        .expect(format!("Couldn't rename {} into {}", filename_tmp, filename).as_str());
}

/// creates a new checkpoint in the file checkpoints
fn create_cp(prefix: &str, cp: &mut HashSet<String>) {
    let mut cp_file = fs::OpenOptions::new()
        .append(true)
        .open(format!("{}/checkpoints", prefix).as_str())
        .expect(format!("cannot open checkpoints file").as_str());
    cp.iter().for_each(|ori| {
        cp_file
            .write(format!("{}\n", ori).as_bytes())
            .expect("couldn't write data in checkpoints");
    });
    cp.clear();
}

/// Load a checkpoint file
/// return the set of origins already parsed if there's a checkpoint
/// None otherwise
pub fn load_checkpoint(opts: &env::Options) -> Option<HashSet<String>> {
    let prefix: &str = opts.results.as_str();
    let mut res = HashSet::new();

    if let Ok(cp_file) = File::open(format!("{}/checkpoints", prefix)) {
        BufReader::new(cp_file)
            .lines()
            .into_iter()
            .for_each(|line| {
                if let Ok(ori) = line {
                    res.insert(ori);
                }
            });
    } else {
        info!("couldn't open checkpoints file");
        return None;
    }

    return Some(res);
}
