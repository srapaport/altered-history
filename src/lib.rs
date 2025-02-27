pub mod analysis;
pub mod classes;
pub mod env;
pub mod orc_gen;
use anyhow::{ensure, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use rayon::ThreadPoolBuilder;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use swh_graph::mph::DynMphf;
//use std::time::Instant;
use swh_graph::graph::*;
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
pub fn altered_commits<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snapshots: &mut VecDeque<String>,
    removed_branch: bool,
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
    // let mut swhid1 = "swh:1:snp:".to_string() + &(snapshots.pop_front().unwrap()); // --> to do like this with rocksdb
    let mut swhid1 = snapshots.pop_front().unwrap();
    debug!("swhid1: {swhid1}");

    let mut swhid1_heads: HashMap<String, usize>;
    match branches_head(&swhid1, graph) {
        None => return None,
        Some(b) => swhid1_heads = b,
    }

    let mut swhid1_commits: HashMap<String, HashSet<usize>> = HashMap::new();

    'find_altered_commits: loop {
        let mut swhid2 = snapshots.pop_front().unwrap();
        while swhid1 == swhid2 {
            if snapshots.len() < 1 {
                break 'find_altered_commits;
            }
            swhid2 = snapshots.pop_front().unwrap();
        }

        let swhid2_heads: HashMap<String, usize>;
        match branches_head(&swhid2, graph) {
            None => return None,
            Some(b) => swhid2_heads = b,
        }

        let mut swhid2_commits: HashMap<String, HashSet<usize>> = HashMap::new();

        for (branch, head1) in swhid1_heads.iter() {
            match swhid2_heads.get(branch) {
                // the branch exists in swhid2
                Some(head2) => {
                    // If the branch changed
                    if *head1 != *head2 {
                        swhid2_commits
                            .insert(branch.clone(), find_all_commits_in_branch(*head2, graph));
                        // If the head is in the commits of snapshot swhid2
                        if swhid2_commits.get(branch).unwrap().contains(head1) {
                            continue;
                        }
                        if swhid1_commits.get(branch).is_none() {
                            swhid1_commits
                                .insert(branch.clone(), find_all_commits_in_branch(*head1, graph));
                        }
                        let difference: HashSet<usize> = swhid1_commits
                            .get(branch)
                            .unwrap()
                            .difference(&swhid2_commits.get(branch).unwrap())
                            .cloned()
                            .collect();
                        // if there np missing commit <=> difference is empty, we jump to the next iteration
                        if difference.is_empty() {
                            continue;
                        }
                        curr_altered_commits.extend(
                            difference
                                .into_iter()
                                .map(|altered_commit| -> (String, String, String, String) {
                                    let swhid_altered_commit =
                                        graph.properties().swhid(altered_commit).to_string();
                                    (
                                        swhid1.clone(),
                                        branch.clone(),
                                        swhid_altered_commit,
                                        swhid2.clone(),
                                    )
                                })
                                .collect::<HashSet<(String, String, String, String)>>(),
                        );
                    }
                }
                // the branch doesn't exist anymore
                None => {
                    // add all commits from the branch
                    // to be categorized in RemovedBranch
                    // if the option removed_branch is true
                    if removed_branch {
                        let all_commits;
                        if swhid1_commits.get(branch).is_none() {
                            swhid1_commits
                                .insert(branch.clone(), find_all_commits_in_branch(*head1, graph));
                        }
                        all_commits = swhid1_commits.get(branch).unwrap();
                        curr_altered_commits.extend(
                            all_commits
                                .into_iter()
                                .map(|altered_commit| -> (String, String, String, String) {
                                    let swhid_altered_commit =
                                        graph.properties().swhid(*altered_commit).to_string();
                                    (
                                        swhid1.clone(),
                                        branch.clone(),
                                        swhid_altered_commit,
                                        swhid2.clone(),
                                    )
                                })
                                .collect::<HashSet<(String, String, String, String)>>(),
                        );
                    }
                }
            }
        }
        if snapshots.len() < 1 {
            break 'find_altered_commits;
        }
        swhid1 = swhid2;
        swhid1_heads = swhid2_heads;
        swhid1_commits = swhid2_commits;
    }
    debug!("Current missing commits: {:?}", curr_altered_commits);
    if curr_altered_commits.len() == 0 {
        return None;
    }
    Some(curr_altered_commits)
}

fn branches_head<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snap_swhid: &String,
    graph: &G,
) -> Option<HashMap<String, usize>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    //let start = Instant::now();
    let props = graph.properties();
    if graph
        .properties()
        .node_type(props.node_id(snap_swhid.as_str()).unwrap())
        != NodeType::Snapshot
    {
        return None;
    }
    let mut heads = HashMap::new();
    //Snapshot id
    let node_id = props.node_id(snap_swhid.as_str()).unwrap();
    //All the successors of the snapshots (Release + Revision)
    let successors = graph.labeled_successors(node_id);
    for (succ, labels) in successors {
        let succ_type = props.node_type(succ);
        //If the successor is not a revision or a release we don't wanna check its children nor add it to the list
        if !(succ_type == NodeType::Revision) && !(succ_type == NodeType::Release) {
            //warn!("A snapshot successor is neither a release nor a revision. Snapshot: {} | Successor: {}", snap_swhid, succ);
            //Probably returning None but can't be sure so continue
            continue;
        }
        let mut head = None;
        if succ_type == NodeType::Revision {
            head = Some(succ);
        } else {
            // If it is a Release then the head is a successor
            let mut to_visit = VecDeque::new();
            to_visit.push_back(succ);
            let mut visited = HashSet::new();
            'visit: while let Some(node) = to_visit.pop_front() {
                if visited.contains(&node) {
                    continue;
                }
                visited.insert(node);
                let successors = graph.successors(node);
                for succ in successors {
                    match props.node_type(succ) {
                        NodeType::Revision => {
                            head = Some(succ);
                            break 'visit;
                        }
                        NodeType::Release => {
                            //info!("RELEASE");
                            to_visit.push_back(succ);
                        }
                        _ => (),
                    }
                }
            }
            if head == None {
                return None;
            }
        }

        for label in labels {
            let branch_name: String;
            if let EdgeLabel::Branch(branch) = label {
                branch_name =
                    String::from_utf8_lossy(&(props.label_name(branch.filename_id()))).to_string();
                heads.insert(branch_name, head.unwrap());
            } else {
                continue;
            }
        }
    }
    if heads.len() == 0 {
        return None;
    }
    //info!("branches_head |  time elapsed: {:.2?}", start.elapsed());
    Some(heads)
}

fn find_all_commits_in_branch<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    head: usize,
    graph: &G,
) -> HashSet<usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    //let start = Instant::now();
    let mut all_commits = HashSet::new();

    let mut nodes_to_visit = Vec::new();
    nodes_to_visit.push(head);
    all_commits.insert(head);
    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(node) = nodes_to_visit.pop() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        let successors = graph.successors(node);
        for succ in successors {
            let succ_type = graph.properties().node_type(succ);
            // We are not visiting the successors of Directories
            if succ_type != NodeType::Revision {
                continue;
            }
            all_commits.insert(succ);
            nodes_to_visit.push(succ);
        }
    }
    //info!("find_all_commits_in_branch | time elapsed: {:.2?}", start.elapsed());
    all_commits
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
/// Start where the last checkpoint stoped
/// Write the results at opts.results
pub fn main_all_mpsc_with_cp(opts: &env::Options, checkpoint_opt: Option<HashSet<String>>) {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let prefix_res: &str = opts.results.as_str();
    let number_of_origins: usize = opts.expected_origins;
    let chunk: usize = opts.chunk;
    let checkpoint: HashSet<String>;
    let removed_branch = opts.removed_branch;
    match checkpoint_opt {
        Some(cp) => checkpoint = cp,
        None => checkpoint = HashSet::new(),
    }
    let (tx, rx) = channel::<
        Option<(
            String,
            String,
            Option<HashSet<(String, String, String, String)>>,
        )>,
    >();
    let jh = thread::spawn(move || {
        let amount_of_ori_kept = AtomicUsize::new(0);
        let amount_of_ori_less_2_visit = AtomicUsize::new(0);
        let workers = (num_cpus::get() / 3) * 2;
        let graph = SwhUnidirectionalGraph::new(graph_path)
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<DynMphf>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_properties(|properties| properties.load_strings())
            .expect("Could no load strings")
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
                    let mut amount_visit = 0;
                    graph
                        .labeled_successors(ori)
                        .into_iter()
                        .for_each(|(_, labels)| {
                            amount_visit += labels.into_iter().count();
                        });
                    if amount_visit <= 1 {
                        amount_of_ori_less_2_visit.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    amount_of_ori_kept.fetch_add(1, Ordering::Relaxed);
                    thread.spawn({
                        let tx = tx.clone();
                        let count_origins = &count_origins;
                        let bar = &bar;
                        let graph = &graph;
                        let checkpoint = &checkpoint;
                        move |_| {
                            let tx = tx;
                            count_origins.fetch_add(1, Ordering::Relaxed);
                            let ori_swhid = graph.properties().swhid(ori).to_string();
                            let thread_id = unsafe { gettid() };
                            ///////// Skipping if necessary according to the checkpoints
                            if !(*checkpoint).contains(&ori_swhid) {
                                if let Some(url) = graph.properties().message(ori) {
                                    let ori_url = String::from_utf8_lossy(&url).to_string();
                                    info!(
                                        "Checking Origin: {} | with pid: {:?}",
                                        ori_swhid, thread_id
                                    );
                                    let sorted_snapshots: Vec<String> =
                                        retrieve_sorted_snapshots(ori, &graph)
                                            .unwrap()
                                            .into_iter()
                                            .map(|(snap, _)| {
                                                graph.properties().swhid(snap).to_string()
                                            })
                                            .filter(|snap| snap != env::EMPTY_SNAPSHOT_SWHID)
                                            .collect();
                                    debug!("    sorted snapshots: {:?}", sorted_snapshots);

                                    ///////////// altered_commits
                                    if let Some(curr_altered_commits) = altered_commits(
                                        &mut VecDeque::from(sorted_snapshots),
                                        removed_branch,
                                        &graph,
                                    ) {
                                        info!(
                                            "Missing commits in {} | with pid: {}",
                                            ori_swhid, thread_id
                                        );
                                        tx.send(Some((
                                            ori_swhid,
                                            ori_url,
                                            Some(curr_altered_commits),
                                        )))
                                        .expect("Failed sending the msg");
                                    } else {
                                        info!(
                                            "No missing commits in {} | with pid: {}",
                                            ori_swhid, thread_id
                                        );
                                        tx.send(Some((ori_swhid, ori_url, None)))
                                            .expect("Failed sending the msg");
                                    }
                                } else {
                                    env::ORI_REJECTED.fetch_add(1, Ordering::Relaxed);
                                    tx.send(Some((ori_swhid, String::new(), None)))
                                        .expect("Failed sending the msg");
                                }
                            } else {
                                info!(
                                    "Not calculating Origin: {} | with pid {}",
                                    ori_swhid, thread_id
                                );
                                tx.send(None).expect("Failed sending the msg");
                            }
                            let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                            bar.set_position(curr_nb_origins);
                        }
                    });
                }
            });
        });
        bar.finish_and_clear();
        println!(
            "Amount of origins kept: {}",
            amount_of_ori_kept.load(Ordering::Relaxed)
        );
        println!(
            "Amount of origins ignored (less than 2 visits): {}",
            amount_of_ori_less_2_visit.load(Ordering::Relaxed)
        );
    });
    // Receive results in parallel and write them at the frequency of opts.chunk
    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    for _ in 0..number_of_origins {
        if let Some(package) = rx.recv().expect("Couldn't receive data") {
            let (ori_swhid, ori_url, value) = package;
            if let Some(mc) = value {
                if res.len() > chunk {
                    flush_map(prefix_res, &mut res);
                    create_cp(prefix_res, &mut cp);
                }
                res.insert(ori_url, mc);
            }
            cp.insert(ori_swhid);
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

/// writes the current results with chunk amount of results in prefix
fn flush_map(prefix: &str, map: &mut HashMap<String, HashSet<(String, String, String, String)>>) {
    let swhid = format!("{}", map.iter().next().unwrap().1.iter().next().unwrap().0);
    let filename = format!("{}/{}.csv", prefix, swhid);
    let filename_tmp = format!("{}/{}.tmp", prefix, swhid);
    if let Ok(_) = fs::metadata(&filename_tmp) {
        warn!(
            "lib.rs > flush_map | File {} already exists!",
            &filename_tmp
        );
        return;
    }
    // let Ok(mut file_w) = File::create_new(format!("{}", filename_tmp)) else {
    //     warn!("File {} already exists!", filename_tmp);
    //     return;
    // };
    let mut csv_wrt = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_path(filename_tmp.as_str())
        .unwrap();
    // file_w
    //     .write("origin;snapshot_src;branch_name;missing_commit;snapshot_dst\n".as_bytes())
    //     .expect(format!("couldn't write headings in file: {}", filename_tmp).as_str());
    map.iter().for_each(|v| {
        v.1.iter().for_each(|set| {
            let record = env::AlteredCommit {
                origin: v.0.to_string(),
                snapshot_src: set.0.to_string(),
                branch_name: set.1.to_string(),
                missing_commit: set.2.to_string(),
                snapshot_dst: set.3.to_string(),
                first_difference: None,
                main_category: None,
                sub_categories: None,
            };
            csv_wrt.serialize(record).unwrap();
            // file_w
            //     .write(format!("{};{};{};{};{}\n", replace_semicolon(v.0), replace_semicolon(&set.0), replace_semicolon(&set.1), replace_semicolon(&set.2), replace_semicolon(&set.3)).as_bytes())
            //     .expect(format!("couldn't write datas in file: {}", filename_tmp).as_str());
        });
    });
    csv_wrt.flush().unwrap();
    map.clear();
    //drop(file_w); // didn't end with this instruction one time
    std::fs::rename(&filename_tmp, &filename)
        .expect(format!("Couldn't rename {} into {}", filename_tmp, filename).as_str());
}

/// creates a new checkpoint in the file checkpoints
fn create_cp(prefix: &str, cp: &mut HashSet<String>) {
    let filename = format!("{}/checkpoints", prefix);
    if let Err(_) = fs::metadata(&filename) {
        File::create_new(&filename)
            .expect(format!("lib.rs > create_cp | File {} already exists!", &filename).as_str());
    }
    let mut cp_file = fs::OpenOptions::new()
        .append(true)
        .open(filename)
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

pub fn main_single_origin(
    opts: &env::Options,
    origin: &str,
) -> Option<HashSet<(String, String, String, String)>> {
    let graph_path = PathBuf::from(opts.graph.as_str());
    let removed_branch = opts.removed_branch;

    let graph = SwhUnidirectionalGraph::new(graph_path)
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_properties(|properties| properties.load_strings())
        .expect("Could no load strings")
        .load_labels()
        .expect("Could not load labels");

    let ori_swhid = format!(
        "swh:1:ori:{}",
        sha1_smol::Sha1::from(origin).digest().to_string()
    );
    let ori_node = graph.properties().node_id(ori_swhid.as_str()).expect(
        format!(
            "couldn't find node id for swhid {} for origin {}",
            ori_swhid, origin
        )
        .as_str(),
    );

    let sorted_snapshots: Vec<String> = retrieve_sorted_snapshots(ori_node, &graph)
        .unwrap()
        .into_iter()
        .map(|(snap, _)| graph.properties().swhid(snap).to_string())
        .filter(|snap| snap != env::EMPTY_SNAPSHOT_SWHID)
        .collect();
    debug!("    sorted snapshots: {:?}", sorted_snapshots);

    ///////////// altered_commits
    if let Some(curr_altered_commits) = altered_commits(
        &mut VecDeque::from(sorted_snapshots),
        removed_branch,
        &graph,
    ) {
        info!("Missing commits in {}", ori_swhid);
        curr_altered_commits.iter().for_each(|ac| {
            println!("Snapshot with altered commit: {}", ac.0);
            println!("Branch name: {}", ac.1);
            println!("Altered commit: {}", ac.2);
            println!("Snapshot without altered commit: {}", ac.3);
            println!("---");
        });
        return Some(curr_altered_commits);
    } else {
        info!("No missing commits in {}", ori_swhid);
    }
    None
}
