use crate::analysis;
use crate::env;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{info, warn};
//use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use swh_graph::graph::*;
use swh_graph::labels::EdgeLabel;
use swh_graph::mph::DynMphf;
use swh_graph::NodeType;

/// returns the (node id, swhid) of the root directory of the given commit
fn get_dir<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    rev_swhid: &str,
    graph: &G,
) -> Option<usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let node_id = graph.properties().node_id(rev_swhid).unwrap();
    for succ in graph.successors(node_id) {
        if graph.properties().node_type(succ) == NodeType::Directory {
            return Some(succ);
        }
    }
    None
}

/// returns the node that contains the given directory in the given snapshot if any
fn is_dir_in_snapshot<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    id_dir: usize,
    snap_swhid: &str,
    graph: &G,
) -> Option<(usize, String)>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let node_snap = graph.properties().node_id(snap_swhid).unwrap();
    let mut to_visit = Vec::new();
    to_visit.push(node_snap);
    let mut visited = HashSet::new();
    while let Some(node) = to_visit.pop() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        let successors = graph.successors(node);

        for succ in successors {
            if graph.properties().node_type(succ) == NodeType::Content {
                continue;
            }
            if id_dir == succ {
                let swhid_parent = graph.properties().swhid(node).to_string();
                return Some((node, swhid_parent));
            }
            to_visit.push(succ);
        }
    }
    None
}

/// return the list of all the content node id associated with their path in the given dir
fn get_list_of_content<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    dir: usize,
    graph: &G,
) -> HashMap<String, usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    // map returned by the function <path of content, node_id>
    let mut res = HashMap::new();

    // map with <node id, path> to build the final path of each content node
    let mut path_node: HashMap<usize, String> = HashMap::new();
    path_node.insert(dir, String::from("."));

    let mut to_visit = VecDeque::new();
    to_visit.push_back(dir);
    let mut visited = HashSet::new();
    while let Some(node) = to_visit.pop_front() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        let successors = graph.labeled_successors(node);
        for (succ, labels) in successors {
            for label in labels {
                let name: String;
                if let EdgeLabel::DirEntry(dir) = label {
                    name =
                        String::from_utf8_lossy(&graph.properties().label_name(dir.filename_id()))
                            .to_string();
                } else {
                    // We just care about labels that are giving the path
                    continue;
                }
                let path = format!(
                    "{}/{}",
                    path_node
                        .get(&node)
                        .expect("couldn't find path in path_node"),
                    name
                );
                match graph.properties().node_type(succ) {
                    NodeType::Content => {
                        res.insert(path, succ);
                    }
                    NodeType::Directory => {
                        path_node.insert(succ, path);
                        to_visit.push_front(succ);
                    }
                    _ => continue,
                }
            }
        }
    }
    res
}

/// Compare the two given map of <path, node id content>.
/// return the map of the content that got altered <(path, id content), categ of alteration>
/// Change this function to count the amount of content modified vs removed
fn compare_dir(
    dir_src: &HashMap<String, usize>,
    dir_dst: &HashMap<String, usize>,
) -> HashMap<(String, usize), Option<env::SubCateg>> {
    let mut res: HashMap<(String, usize), Option<env::SubCateg>> = HashMap::new();
    let mut modified = false;
    let mut removed = false;
    for (path, cnt_id) in dir_src.into_iter() {
        match dir_dst.get(path) {
            Some(node_id) => {
                if node_id == cnt_id {
                    res.insert((path.to_owned(), cnt_id.to_owned()), None);
                } else {
                    res.insert(
                        (path.to_owned(), cnt_id.to_owned()),
                        Some(env::SubCateg::FileModified),
                    );
                    modified = true;
                }
            }
            None => {
                res.insert(
                    (path.to_owned(), cnt_id.to_owned()),
                    Some(env::SubCateg::FileRemoved),
                );
                removed = true;
            }
        }
        if modified && removed {
            break;
        }
    }
    // To modify if I want to count each amount
    // dir_src
    //     .iter()
    //     .for_each(|(path, cnt_id)| match dir_dst.get(path) {
    //         Some(node_id) => {
    //             if node_id == cnt_id {
    //                 res.insert((path.to_owned(), cnt_id.to_owned()), None);
    //             } else {
    //                 res.insert(
    //                     (path.to_owned(), cnt_id.to_owned()),
    //                     Some(env::SubCateg::FileModified),
    //                 );
    //             }
    //         }
    //         None => {
    //             res.insert(
    //                 (path.to_owned(), cnt_id.to_owned()),
    //                 Some(env::SubCateg::FileRemoved),
    //             );
    //         }
    //     });
    res
}

fn update_changes(
    map: &mut HashMap<(String, usize), Option<env::SubCateg>>,
    changes: HashMap<(String, usize), Option<env::SubCateg>>,
) {
    changes.into_iter().for_each(|(key, categ)| {
        match map.get(&key) {
            Some(opt) => {
                match opt {
                    Some(cat) => {
                        match cat {
                            env::SubCateg::FileRemoved => {
                                map.insert(key, categ);
                            }
                            env::SubCateg::FileModified => {
                                match categ {
                                    None => {
                                        map.insert(key, categ); //We modify FileModified into None because we found the content as it was in a revision
                                    }
                                    Some(_) => (), //if the content is removed or modified we don't change it
                                }
                            }
                            _ => warn!("shouldn't be something else than removed or modified"),
                        }
                    }
                    None => (), //Don't modify, the content was found in some revision
                }
            }
            None => {
                map.insert(key, categ);
            }
        }
    });
}

/// Classify the meta changes in the missing commit
fn meta_changes<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: &str,
    first_difference: usize,
    graph: &G,
) -> env::Categ
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut categ = env::Categ {
        main_categ: env::MainCateg::META,
        sub_categ: HashSet::new(),
    };

    let missing_commit = graph
        .properties()
        .node_id(missing_commit)
        .expect("couldn't find node id");

    if message_diff(missing_commit, first_difference, graph) {
        categ.sub_categ.insert(env::SubCateg::Message);
    }
    if author_diff(missing_commit, first_difference, graph) {
        categ.sub_categ.insert(env::SubCateg::Author);
    }
    if committer_diff(missing_commit, first_difference, graph) {
        categ.sub_categ.insert(env::SubCateg::Committer);
    }
    if date_diff(missing_commit, first_difference, graph) {
        categ.sub_categ.insert(env::SubCateg::Date);
    }
    if committer_date_diff(missing_commit, first_difference, graph) {
        categ.sub_categ.insert(env::SubCateg::CommitterDate);
    }
    categ
}

fn message_diff<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    first_difference: usize,
    graph: &G,
) -> bool
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
{
    let msg_mc = graph.properties().message(missing_commit);
    let msg_fd = graph.properties().message(first_difference);
    if let Some(msg) = msg_mc {
        if let Some(msg2) = msg_fd {
            let msg = String::from_utf8_lossy(&msg).to_string();
            let msg2 = String::from_utf8_lossy(&msg2).to_string();
            return msg != msg2;
        }
        return true;
    }
    if let Some(_) = msg_fd {
        return true;
    }
    false
}

fn author_diff<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    first_difference: usize,
    graph: &G,
) -> bool
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
{
    let author_md = graph.properties().author_id(missing_commit);
    let author_fd = graph.properties().author_id(first_difference);
    if let Some(author) = author_md {
        if let Some(author2) = author_fd {
            return author != author2;
        }
        return true;
    }
    if let Some(_) = author_fd {
        return true;
    }
    false
}

fn committer_diff<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    first_difference: usize,
    graph: &G,
) -> bool
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
{
    let committer_md = graph.properties().committer_id(missing_commit);
    let committer_fd = graph.properties().committer_id(first_difference);
    if let Some(committer) = committer_md {
        if let Some(committer2) = committer_fd {
            return committer != committer2;
        }
        return true;
    }
    if let Some(_) = committer_fd {
        return true;
    }
    false
}

fn date_diff<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    first_difference: usize,
    graph: &G,
) -> bool
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let date_md = graph.properties().author_timestamp(missing_commit);
    let date_fd = graph.properties().author_timestamp(first_difference);
    if let Some(date) = date_md {
        if let Some(date2) = date_fd {
            return date != date2;
        }
        return true;
    }
    if let Some(_) = date_fd {
        return true;
    }
    false
}

fn committer_date_diff<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    first_difference: usize,
    graph: &G,
) -> bool
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let committer_date_md = graph.properties().committer_timestamp(missing_commit);
    let committer_date_fd = graph.properties().committer_timestamp(first_difference);
    if let Some(committer_date) = committer_date_md {
        if let Some(committer_date2) = committer_date_fd {
            return committer_date != committer_date2;
        }
        return true;
    }
    if let Some(_) = committer_date_fd {
        return true;
    }
    false
}

fn dir_changes<G: SwhLabeledForwardGraph + SwhGraphWithProperties + SwhLabeledBackwardGraph>(
    dir: HashMap<String, usize>,
    rev_swhid: &str,
    snap_dst: &str,
    branch: &str,
    graph_t: &G,
) -> Option<HashMap<(String, usize), Option<env::SubCateg>>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut res = HashMap::new();
    let props = graph_t.properties();

    let snap_id = props
        .node_id(snap_dst)
        .expect(&format!("couldn't find {} in the graph", snap_dst));
    //Check if the branch still exists
    let mut rev_id: Option<usize> = None;
    'global: for (succ, labels) in graph_t.labeled_successors(snap_id) {
        for label in labels {
            let curr_branch: String;
            if let EdgeLabel::Branch(b) = label {
                curr_branch =
                    String::from_utf8_lossy(&props.label_name(b.filename_id())).to_string();
            } else {
                continue;
            }
            if &curr_branch == branch {
                rev_id = Some(succ);
                break 'global;
            }
        }
    }
    let Some(_) = rev_id else {
        return None; //all the changes are SubCateg::REMOVED --> branch disapeared
    };

    // node id of altered commit
    let rev_mc: usize = graph_t
        .properties()
        .node_id(rev_swhid)
        .expect("Couldn't find node id");

    let mut succs = HashSet::new();
    // find all successors commits or None is it's the initial commit
    // successors should be in snap_dst since we focused on root cause commits
    // if there is no successor, then we compare it with the root commit of snap_dst
    match find_successors(rev_mc, graph_t) {
        Some(findings) => {
            succs.extend(findings);
        }
        None => {
            succs.insert(find_initial_rev(snap_dst, graph_t).expect("Can't find initial commit"));
        }
    }

    match find_revs(rev_mc, succs, graph_t) {
        Some(rev_to_explore) => {
            rev_to_explore.into_iter().for_each(|rev| {
                if let Some(dir_dst) = get_dir(props.swhid(rev).to_string().as_str(), graph_t) {
                    let changes = compare_dir(&dir, &get_list_of_content(dir_dst, graph_t));
                    update_changes(&mut res, changes);
                } else {
                    warn!("couldn't find dir for rev {}", props.swhid(rev).to_string());
                    env::REV_WITHOUT_DIR.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        None => {
            //If we have no revision to visit / to compare the missing commit to
            res.insert((String::new(), 0), Some(env::SubCateg::FileRemoved));
        }
    }

    Some(res)
}

fn get_list_of_changes(
    changes: HashMap<(String, usize), Option<env::SubCateg>>,
) -> HashSet<env::SubCateg> {
    let mut res: HashSet<env::SubCateg> = HashSet::new();
    for (_, v) in changes.into_iter() {
        if let Some(categ) = v {
            res.insert(categ);
        }
        if res.len() > 1 {
            //Meaning Modified and removed are already in there
            return res;
        }
    }
    if res.len() == 0 {
        res.insert(env::SubCateg::ContentSplit);
    }
    res
}

fn find_initial_rev<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snap_swhid: &str,
    graph: &G,
) -> Option<usize>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let snap_id = graph
        .properties()
        .node_id(snap_swhid)
        .expect("Couldn't find id for rev");
    let mut to_visit = Vec::new();
    to_visit.push(snap_id);
    let mut visited = HashSet::new();
    while let Some(node) = to_visit.pop() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        let mut amount_of_revs = 0;
        graph.successors(node).into_iter().for_each(|succ| {
            match graph.properties().node_type(succ) {
                NodeType::Revision => {
                    to_visit.push(succ);
                    amount_of_revs += 1;
                }
                _ => (),
            }
        });
        if amount_of_revs == 0 {
            //Initial commit
            return Some(node);
        }
    }
    None
}

fn find_successors<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    rev_id: usize,
    graph: &G,
) -> Option<HashSet<usize>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut res = HashSet::new();

    graph.successors(rev_id).into_iter().for_each(|node| {
        match graph.properties().node_type(node) {
            NodeType::Revision => {
                res.insert(node);
            }
            _ => (),
        }
    });

    if res.len() < 1 {
        //the rev is the initial commit
        return None;
    }
    Some(res)
}

/// return all the commits predecessors of the altered commits.
/// goes to a max depth of `env::MAX_DEPTH`
fn find_revs<G: SwhLabeledBackwardGraph + SwhGraphWithProperties>(
    missing_commit: usize,
    succs: HashSet<usize>,
    graph_t: &G,
) -> Option<HashSet<usize>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut res = HashSet::new();

    succs.into_iter().for_each(|succ| {
        let mut visit_pred_curr = HashSet::new();
        let mut visit_pred_next = HashSet::new();
        visit_pred_curr.insert(succ);
        for _ in 0..env::MAX_DEPTH {
            visit_pred_curr.into_iter().for_each(|node| {
                graph_t.predecessors(node).into_iter().for_each(|pred| {
                    match graph_t.properties().node_type(pred) {
                        NodeType::Revision => {
                            if pred != missing_commit {
                                res.insert(pred);
                                visit_pred_next.insert(pred);
                            }
                        }
                        _ => (),
                    }
                });
            });
            visit_pred_curr = visit_pred_next.clone();
            if visit_pred_curr.len() == 0 {
                break; //No need to go deeper
            }
            visit_pred_next.clear();
        }
    });
    if res.len() == 0 {
        return None;
    }
    Some(res)
}

/// Classify all the missing commits of a file
fn file_classification<
    G: SwhLabeledForwardGraph + SwhGraphWithProperties + SwhLabeledBackwardGraph + Sync,
>(
    filename: &str,
    opts: &env::Options,
    graph_t: &G,
    progress: &MultiProgress,
) -> Option<HashMap<(String, String, String, String, String, Option<String>), env::Categ>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut res = HashMap::new();

    let file_results = analysis::load_file_results(opts, filename)
        .expect(&format!("couldn't load results from {}", filename));

    let (tx, rx) = channel::<
        Option<(
            (String, String, String, String, String, Option<String>),
            env::Categ,
        )>,
    >();
    let length = file_results.len();
    let bar = progress.add(ProgressBar::new(length as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message(format!("Classifying {}", filename));
    file_results.into_par_iter().for_each(|line| {
        let tx = tx.clone();
        let package = line_classification(line.clone(), graph_t);
        tx.send(package)
            .expect(format!("Failed sending the msg to classify the commit {:?}", line.3).as_str());
        bar.inc(1);
    });
    for _ in 0..length {
        if let Ok(package) = rx.recv() {
            if let Some(data) = package {
                res.insert(data.0, data.1);
            }
        }
    }
    bar.finish_with_message(format!("Done Classifying {}", filename));
    progress.remove(&bar);
    return Some(res);
}

/// Classify a single missing commit
fn line_classification<
    G: SwhLabeledForwardGraph + SwhGraphWithProperties + SwhLabeledBackwardGraph,
>(
    //res: &mut HashMap<(String, String, String, String, String, Option<String>), env::Categ>,
    line: (String, String, String, String, String),
    graph_t: &G,
) -> Option<(
    (String, String, String, String, String, Option<String>),
    env::Categ,
)>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut categ: env::Categ;

    if let Some(dir) = get_dir(&line.3, graph_t) {
        let first_difference = is_dir_in_snapshot(dir, &line.4, graph_t);
        match first_difference {
            Some((fd, fd_swhid)) => {
                //META changes
                if fd_swhid == line.3 {
                    categ = env::Categ {
                        main_categ: env::MainCateg::DIR,
                        sub_categ: HashSet::new(),
                    };
                    categ.sub_categ.insert(env::SubCateg::DifferentBranchName);
                } else {
                    categ = meta_changes(&line.3, fd, graph_t);
                }
                // res.insert(
                //     (line.0, line.1, line.2, line.3, line.4, Some(fd_swhid)),
                //     categ,
                // );
                return Some((
                    (line.0, line.1, line.2, line.3, line.4, Some(fd_swhid)),
                    categ,
                ));
            }
            None => {
                //DIR changes
                categ = env::Categ {
                    main_categ: env::MainCateg::DIR,
                    sub_categ: HashSet::new(),
                };
                let lc = get_list_of_content(dir, graph_t);
                let changes = dir_changes(lc, &line.3, &line.4, &line.2, graph_t);
                match changes {
                    Some(changes) => {
                        categ.sub_categ = get_list_of_changes(changes);
                    }
                    None => {
                        categ.sub_categ.insert(env::SubCateg::RemovedBranch); // only usefull if removed_branch is true
                    }
                }
                //res.insert((line.0, line.1, line.2, line.3, line.4, None), categ);
                return Some(((line.0, line.1, line.2, line.3, line.4, None), categ));
            }
        }
    } else {
        warn!("couldn't find dir for rev {}", line.3.as_str());
        env::REV_WITHOUT_DIR.fetch_add(1, Ordering::Relaxed);
        // categ = env::Categ {
        //     main_categ: env::MainCateg::LoadingIssue,
        //     sub_categ: HashSet::new(),
        // };
        // res.insert((line.0, line.1, line.2, line.3, line.4, None), categ);
        return None;
    }
}

extern "C" {
    fn gettid() -> u32;
}

/// Classify all altered commits from results files in focus
pub fn classification_all(opts: &env::Options) -> bool {
    let prefix: String = format!("{}/focus", opts.results);
    let graph_name: String = opts.graph.clone();

    let graph_t = SwhBidirectionalGraph::new(PathBuf::from(graph_name))
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

    // let workers = (num_cpus::get() / 3) * 2;
    // let pool = ThreadPoolBuilder::new()
    //     .num_threads(workers)
    //     .build()
    //     .unwrap();

    let entries = fs::read_dir(prefix.clone()).expect("can't read dir");
    let multi_bar = MultiProgress::new();
    let bar_file = multi_bar.add(ProgressBar::new((entries.count() - 1) as u64));
    bar_file.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    bar_file.set_message("Amount of Files Classified");
    let count_file = AtomicUsize::new(0);
    //pool.install(|| {
    //rayon::scope(|thread| {
    fs::read_dir(prefix.clone())
        .expect("can't read dir")
        .into_iter()
        .for_each(|file| {
            //thread.spawn(|_| {
            let filename = &file
                .unwrap()
                .path()
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap();
            info!("file class: {}", filename);
            if env::RE_CSV.is_match(&filename) {
                info!("parsing: {}", filename);
                let name = &env::RE_FILENAME_WITHOUT_EXT
                    .captures(filename)
                    .expect("couldn't etract name of the csv file")[1];
                let filename_dst = format!("{}/classes/{}_class.csv", prefix, name);
                match fs::metadata(&filename_dst) {
                    Ok(_) => warn!(
                        "classes.rs > classification_all | File {} already exists",
                        filename_dst
                    ), //file already exists -> do nothing
                    Err(_) => {
                        let thread_id = unsafe { gettid() };
                        info!(
                            "Computing file: {} | with pid: {:?}",
                            format!("focus/{}", filename),
                            thread_id
                        );
                        if let Some(p) = file_classification(
                            &format!("focus/{}", filename),
                            opts,
                            &graph_t,
                            &multi_bar,
                        ) {
                            save_categ(opts, &filename_dst, p, &multi_bar);
                        }
                    }
                }
            }
            count_file.fetch_add(1, Ordering::Relaxed);
            let curr_nb_file = count_file.load(Ordering::Relaxed) as u64;
            bar_file.set_position(curr_nb_file);
            //});
        });
    //});
    //});

    bar_file.finish_with_message("Finished Classifying all Files");
    true
}

fn save_categ(
    opts: &env::Options,
    filename: &str,
    map: HashMap<(String, String, String, String, String, Option<String>), env::Categ>,
    progress: &MultiProgress,
) {
    let prefix = format!("{}/focus/classes", opts.results);
    //Check if directory exists already
    if let Err(_) = fs::metadata(&prefix) {
        fs::create_dir(&prefix).expect(format!("couldn't create directory: {}", &prefix).as_str());
        //Create new directory
    }

    if let Ok(_) = fs::metadata(filename) {
        warn!(
            "classes.rs > save_categ | File {} already exists!",
            filename
        );
        return;
    }
    // let Ok(mut file_w) = File::create_new(filename) else {
    //     warn!("File class {} already exists!", filename);
    //     return;
    // };
    let length = map.len();
    let bar = progress.add(ProgressBar::new(length as u64));
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message(format!("Saving Classified Data in {}", filename));
    let (tx, rx) = channel::<env::AlteredCommit>();
    let mut csv_wrt = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_path(filename)
        .unwrap();
    // file_w
    //     .write("origin;snapshot_src;branch_name;missing_commit;snapshot_dst;first_difference;main_category;sub_categories\n".as_bytes())
    //     .expect(format!("couldn't write headings in file: {}", filename).as_str());
    map.into_par_iter().for_each(|(k, v)| {
        let record = env::AlteredCommit {
            origin: k.0,
            snapshot_src: k.1,
            branch_name: k.2,
            missing_commit: k.3.clone(),
            snapshot_dst: k.4,
            first_difference: k.5,
            main_category: Some(v.main_categ),
            sub_categories: {
                let mut subcateg = String::new();
                v.sub_categ.into_iter().for_each(|sub| {
                    subcateg = format!("{},{}", subcateg, sub.to_string());
                });
                Some(subcateg)
            },
        };
        let tx = tx.clone();
        tx.send(record).expect(
            format!(
                "failed sending the record for classifying {} in file {}",
                k.3, filename
            )
            .as_str(),
        );
        bar.inc(1);
        // file_w
        //     .write(
        //         format!(
        //             "{};{};{};{};{};{:?};{};{:?}\n",
        //             k.0, k.1, k.2, k.3, k.4, k.5, v.main_categ, v.sub_categ
        //         )
        //         .as_bytes(),
        //     )
        //     .expect(format!("couldn't write datas in file: {}", filename).as_str());
    });
    for _ in 0..length {
        if let Ok(package) = rx.recv() {
            let mc = package.missing_commit.clone();
            csv_wrt.serialize(package).expect(
                format!(
                    "Couldn't serialize classified data {} in file {}",
                    mc, filename
                )
                .as_str(),
            );
        }
    }
    bar.finish_with_message(format!("Done Saving Classified Data in {}", filename));
    progress.remove(&bar);
}

pub fn classification_single_origin(
    opts: &env::Options,
    roots: HashSet<(String, String, String, String, String)>,
) -> HashMap<(String, String, String, String, String, Option<String>), env::Categ> {
    let graph_name: String = opts.graph.clone();

    let graph_t = SwhBidirectionalGraph::new(PathBuf::from(graph_name))
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

    let mut res = HashMap::new();
    roots.into_iter().for_each(|root| {
        if let Some(p) = line_classification(root, &graph_t) {
            res.insert(p.0, p.1);
        }
    });
    println!("------ Classes ------");
    res.iter().for_each(|(commit, categ)| {
        println!("Snapshot with altered commit: {}", commit.1);
        println!("Branch name: {}", commit.2);
        println!("Altered commit: {}", commit.3);
        println!("Snapshot without altered commit: {}", commit.4);
        println!("First Diff: {:?}", commit.5);
        println!("Main categ: {}", categ.main_categ);
        println!("Sub categ: {:?}", categ.sub_categ);
        println!("---");
    });
    return res;
}
