use crate::env;
use indicatif::{ProgressBar, ProgressStyle};
use log::{info, warn};
use rayon::ThreadPoolBuilder;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use swh_graph::graph::*;
use swh_graph::java_compat::mph::gov::GOVMPH;

/// For a set of missing commits, extract the ones that don't have any parents in this set
/// return Set(origin, snapshot source, branch name, focused commit, snapshot dest)
fn focus_missing_commits<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    missing_commits: &'a HashSet<(String, String, String, String, String)>,
    graph: &G,
    save: Option<(&env::Options, &str)>,
) -> HashSet<(String, String, String, String, String)>
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
        }
    }
    res
}

/// save the results given in the file filename
/// in the directory
/// if the directory doesn't exist, create it
fn save_focus_commits(
    opts: &env::Options,
    filename: &str,
    res: &HashSet<(String, String, String, String, String)>,
) -> bool {
    let prefix = format!("{}/focus", opts.results);
    //Check if directory exists already
    match fs::metadata(&prefix) {
        Ok(_) => (), //Do nothing
        Err(_) => {
            fs::create_dir(&prefix)
                .expect(format!("couldn't create directory: {}", &prefix).as_str());
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

/// Main work for extracting only the commits that were manually changed for sure
/// Only creates file results if they don't already exists -> files act like checkpoints
/// returns true when ends
pub fn focus_missing_commits_all_files_with_save(opts: &env::Options) -> bool {
    let prefix: &str = opts.results.as_str();
    let graph_name: &str = opts.graph.as_str();

    let graph = SwhUnidirectionalGraph::new(PathBuf::from(graph_name))
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

    let entries = fs::read_dir(prefix).expect("can't read dir");
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
            fs::read_dir(prefix)
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

/// load the data from a csv file
/// the csv file must have the following elements:
/// origin, snapshot source, branch name, altered commit, snapshot destination
pub fn load_file_results(
    opts: &env::Options,
    filename: &str,
) -> Option<HashSet<(String, String, String, String, String)>> {
    let prefix: &str = opts.results.as_str();
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
    return Some(res);
}
