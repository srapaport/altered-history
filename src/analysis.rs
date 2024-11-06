use crate::env;
use indicatif::{HumanCount, ProgressBar, ProgressStyle};
use log::{info, warn};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use swh_graph::graph::*;
use swh_graph::java_compat::mph::gov::GOVMPH;
use swh_graph::labels::{Branch, EdgeLabel};

pub fn focus_missing_commits<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commits: &'a HashSet<(String, String, String, String, String)>,
    graph: &G,
    save: Option<(&env::Options, &str)>,
) -> HashSet<(String, String, String, String, String)>
// return Set(origin, snapshot source, branch name, focused commit, snapshot dest)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let rev_only: HashSet<String> = missing_commits
        .into_iter()
        .map(|commit| commit.3.clone())
        .collect();
    let res = missing_commits
        .iter()
        .filter(|commit| {
            let node_id = graph.properties().node_id(commit.3.as_str()).unwrap();
            for succ in graph.successors(node_id) {
                let swhid = graph.properties().swhid(succ).to_string();
                if rev_only.contains(&swhid) {
                    return false;
                }
            }
            true
        })
        .map(|commit| commit.clone())
        .collect();

    match save {
        None => return res,
        Some((opts, filename)) => {
            if !save_focus_commits(opts, filename, &res) {
                warn!("couldn't save focus commits");
            }
        } //save_focus_commits
    }
    res
}

pub fn save_focus_commits(
    opts: &env::Options,
    filename: &str,
    res: &HashSet<(String, String, String, String, String)>,
) -> bool {
    let prefix: String;
    match opts.dataset.as_str() {
        "2021" => prefix = format!("{}/focus", env::PREFIX_RESULTS_2021),
        "2023" => todo!(),
        "FULL" => prefix = format!("{}/focus", env::PREFIX_RESULTS_FULL),
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        }
    }
    //Check if directory exists already
    match fs::metadata(&prefix) {
        Ok(_) => (), //Do nothing
        Err(_) => {
            fs::create_dir(&prefix).expect(format!("couldn't create dire: {}", prefix).as_str());
        } //Create new directory
    }

    //save res in file
    let name = &env::RE_FILENAME_WITHOUT_EXT
        .captures(filename)
        .expect("couldn't etract name of the csv file")[1];
    let filename = format!("{}_focus.csv", name);
    let Ok(mut file_w) = File::create_new(format!("{}/{}", prefix, filename)) else {
        warn!("File {} already exists!", filename);
        return false;
    };
    file_w
        .write("origin;snapshot_src;branch_name;missing_commit;snapshot_dst\n".as_bytes())
        .expect(format!("couldn't write headings in file: {}", filename).as_str());
    res.iter().for_each(|v| {
        file_w
            .write(format!("{};{};{};{};{}\n", v.0, v.1, v.2, v.3, v.4).as_bytes())
            .expect(format!("couldn't write datas in file: {}", filename).as_str());
    });
    return true;
}

pub fn focus_missing_commits_all_files_with_save(opts: &env::Options) -> bool {
    let prefix: String;
    let basename: String;
    match opts.dataset.as_str() {
        "2021" => {
            prefix = env::PREFIX_RESULTS_2021.to_string();
            basename = env::BASENAME_2021.to_string();
        }
        "2023" => todo!(),
        "FULL" => {
            prefix = env::PREFIX_RESULTS_FULL.to_string();
            basename = env::BASENAME_FULL.to_string();
        }
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        }
    }

    let graph = load_unidirectional(PathBuf::from(&basename))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<GOVMPH>())
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

    let workers = num_cpus::get() / 3;
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();

    let entries = fs::read_dir(prefix.clone()).expect("can't read dir");
    let bar = ProgressBar::new((entries.count() - 1) as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}",
        )
        .unwrap(),
    );
    let count_file = AtomicUsize::new(0);

    pool.install(|| {
        rayon::scope(|thread| {
            fs::read_dir(prefix.clone())
                .expect("can't read dir")
                .into_iter()
                .for_each(|file| {
                    thread.spawn(|_| {
                        let filename = &file
                            .unwrap()
                            .path()
                            .file_name()
                            .unwrap()
                            .to_os_string()
                            .into_string()
                            .unwrap();
                        if env::RE_CSV.is_match(&filename) {
                            let name = &env::RE_FILENAME_WITHOUT_EXT
                                .captures(filename)
                                .expect("couldn't etract name of the csv file")[1];
                            let filename_dst = format!("{}/focus/{}_focus.csv", prefix, name);
                            match fs::metadata(&filename_dst) {
                                Ok(_) => info!("file: {} already exists", filename), //file already exists -> do nothing
                                Err(_) => {
                                    focus_missing_commits(
                                        &load_file_results(&opts, &filename).unwrap(),
                                        &graph,
                                        Some((opts, &filename)),
                                    );
                                }
                            }
                        }
                        count_file.fetch_add(1, Ordering::Relaxed);
                        let curr_nb_file = count_file.load(Ordering::Relaxed) as u64;
                        bar.set_position(curr_nb_file);
                    });
                });
        });
    });
    bar.finish_and_clear();
    true
}

pub fn common_revisions<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commits: HashSet<(String, String, String, String, String)>,
    graph: &G,
) -> HashMap<(String, String, String, String, String), HashSet<(String, String, String)>>
// return Set(snapshot source, common commit, snapshot dest)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut res = HashMap::new();
    missing_commits.into_iter().for_each(|commit| {
        let mut no_common: bool = true;
        graph
            .successors(graph.properties().node_id(commit.3.as_str()).unwrap())
            .into_iter()
            .for_each(|succ| {
                let swhid = graph.properties().swhid(succ).to_string();
                if env::RE_REV.is_match(&swhid) {
                    no_common = false;
                    res.entry((
                        commit.0.to_owned(),
                        commit.1.to_owned(),
                        commit.2.to_owned(),
                        commit.3.to_owned(),
                        commit.4.to_owned(),
                    ))
                    .or_insert_with(HashSet::new)
                    .insert((commit.1.clone(), swhid, commit.4.clone()));
                }
            });
        if no_common {
            res.entry((
                commit.0.to_owned(),
                commit.1.to_owned(),
                commit.2.to_owned(),
                commit.3.to_owned(),
                commit.4.to_owned(),
            ))
            .or_insert_with(HashSet::new)
            .insert(("None".to_string(), "None".to_string(), "None".to_string()));
        }
    });
    return res;
}

pub fn first_revision_different<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    map_common_commits: HashMap<
        (String, String, String, String, String),
        HashSet<(String, String, String)>,
    >,
    graph: &G,
) -> HashMap<
    (String, String, String, String, String),
    HashMap<(String, String, String), HashSet<String>>,
>
// return Set(diff revision)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut ret_value: HashMap<
        (String, String, String, String, String),
        HashMap<(String, String, String), HashSet<String>>,
    > = HashMap::new();

    for (missing, common_commits) in map_common_commits.iter() {
        let branch = missing.2.to_owned();
        let snapshot_dst: String = missing.4.to_owned();

        //Recup first rev of the right branch (labelled successors)...
        let mut node = graph.properties().node_id(snapshot_dst.as_str()).unwrap();
        let mut starting_node: Option<usize> = None;
        graph
            .labeled_successors(node)
            .into_iter()
            .for_each(|(succ, labels)| {
                labels.into_iter().for_each(|label| {
                    if let EdgeLabel::Branch(curr_branch) = label {
                        let branch_name = String::from_utf8_lossy(
                            &(graph.properties().label_name(curr_branch.filename_id())),
                        )
                        .to_string();
                        if branch == branch_name {
                            starting_node = Some(succ);
                        }
                    }
                    /* let filename = graph.properties().label_name(label.filename_id());
                    let branch_name = String::from_utf8_lossy(&filename).to_string();
                    if branch == branch_name{
                        starting_node = Some(succ);
                    } */
                });
            });
        if starting_node == None {
            common_commits.into_iter().for_each(|common| {
                ret_value
                    .entry(missing.to_owned())
                    .or_insert_with(HashMap::new)
                    .entry(common.to_owned())
                    .or_insert_with(HashSet::new)
                    .insert("initial_commit".to_string());
            });
            continue;
        }

        node = starting_node.unwrap();
        let common_commits_rev: HashSet<String> =
            common_commits.into_iter().map(|c| c.1.to_owned()).collect();

        if common_commits_rev.contains("None") {
            common_commits.into_iter().for_each(|common| {
                if common.1 == "None".to_string() {
                    ret_value
                        .entry(missing.to_owned())
                        .or_insert_with(HashMap::new)
                        .entry(common.to_owned())
                        .or_insert_with(HashSet::new)
                        .insert("initial_commit".to_string());
                }
            });
        }

        let mut to_visit = Vec::new();
        let mut visited = Vec::new();
        to_visit.push(node);

        while let Some(node) = to_visit.pop() {
            if visited.contains(&node) {
                continue;
            }
            visited.push(node);

            graph
                .successors(node)
                .into_iter()
                .map(|s| {
                    to_visit.push(s);
                    graph.properties().swhid(s).to_string()
                })
                .into_iter()
                .for_each(|s| {
                    let swhid = graph.properties().swhid(node).to_string();
                    if env::RE_REV.is_match(&swhid) {
                        if common_commits_rev.contains(&s) {
                            for common in common_commits {
                                ret_value
                                    .entry(missing.to_owned())
                                    .or_insert_with(HashMap::new)
                                    .entry(common.to_owned())
                                    .or_insert_with(HashSet::new)
                                    .insert(swhid.to_owned());
                            }
                        }
                    }
                });
        }
    }

    return ret_value;
}

pub fn first_directory<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    swhid_rev: String,
    graph: &G,
) -> Option<String>
// return Set(diff revision)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let node = graph.properties().node_id(swhid_rev.as_str()).unwrap();
    for succ in graph.successors(node) {
        let swhid_succ = graph.properties().swhid(succ).to_string();
        if env::RE_DIR.is_match(&swhid_succ) {
            return Some(swhid_succ);
        }
    }
    return None;
}

pub fn all_oldest_commits<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    opts: &env::Options,
    graph: &G,
) -> HashSet<(String, String, String, String, String)>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut res: HashSet<(String, String, String, String, String)> = HashSet::new();
    for ori in env::ORIGINS_LIST_2021.iter() {
        focus_missing_commits(
            &crate::main_targeted_origin(&opts, ori)
                .unwrap()
                .into_iter()
                .map(|commit| {
                    //println!("commit: {}",commit.2);
                    (ori.to_string(), commit.0, commit.1, commit.2, commit.3)
                })
                .collect::<HashSet<(String, String, String, String, String)>>(),
            &graph,
            None,
        )
        .into_iter()
        .for_each(|commit| {
            println!(
                "{};{};{};{};{}",
                commit.0, commit.1, commit.2, commit.3, commit.4
            );
            res.insert(commit);
        });
    }
    return res;
}

pub fn load_file_results(
    opts: &env::Options,
    filename: &str,
) -> Option<HashSet<(String, String, String, String, String)>> {
    let prefix: &str;
    match opts.dataset.as_str() {
        "2021" => prefix = env::PREFIX_RESULTS_2021,
        "2023" => todo!(),
        "FULL" => prefix = env::PREFIX_RESULTS_FULL,
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        }
    }
    let start = Instant::now();
    let mut res = HashSet::new();

    if !env::RE_CSV.is_match(filename) {
        info!("not loading tmp file: {filename}");
        return None;
    }

    info!("loading res from file: {filename}");
    csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_path(format!("{prefix}/{filename}"))
        .unwrap()
        .records()
        .into_iter()
        .for_each(|result| {
            let srecord = result.unwrap();
            let origin = srecord.get(0).unwrap().to_owned();
            let snapshot_src = srecord.get(1).unwrap().to_owned();
            let branch_name = srecord.get(2).unwrap().to_owned();
            let missing_commit = srecord.get(3).unwrap().to_owned();
            let snapshot_dst = srecord.get(4).unwrap().to_owned();
            res.insert((
                origin,
                snapshot_src,
                branch_name,
                missing_commit,
                snapshot_dst,
            ));
        });

    //info!("Loading file | Time elapsed: {:.2?}", start.elapsed());
    return Some(res);
}
