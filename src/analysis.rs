use crate::env;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{info, warn};
//use rayon::ThreadPoolBuilder;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use swh_graph::graph::*;
use swh_graph::mph::DynMphf;
use rayon::prelude::*;
use std::sync::mpsc::channel;

/// For a set of missing commits, extract the ones that don't have any parents in this set
/// return Set(origin, snapshot source, branch name, focused commit, snapshot dest)
fn focus_missing_commits<'a, G: SwhLabeledForwardGraph + SwhGraphWithProperties + Sync>(
    missing_commits: &'a HashSet<(String, String, String, String, String)>,
    graph: &G,
    save: Option<(&env::Options, &str)>,
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
    bar.set_style(ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap());
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
    match save {
        None => return res,
        Some((opts, filename)) => {
            if !save_focus_commits(opts, filename, &res, progress) {
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
    progress: &MultiProgress,
) -> bool {
    let prefix = format!("{}/focus", opts.results);
    //Check if directory exists already
    if let Err(_) = fs::metadata(&prefix) {
        fs::create_dir(&prefix)
            .expect(format!("couldn't create directory: {}", &prefix).as_str());
        //Create new directory
    }
    //save res in file
    let name = &env::RE_FILENAME_WITHOUT_EXT
        .captures(filename)
        .expect("couldn't etract name of the csv file")[1];
    let filename = format!("{}_focus.csv", name);
    if let Ok(_) = fs::metadata(format!("{}/{}", prefix, filename)){
        warn!("analysis.rs > save_focus_commit | File {} already exists!", format!("{}/{}", prefix, filename));
        return false;
    }
    let bar = progress.add(ProgressBar::new(res.len() as u64));
    bar.set_style(ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap());
    bar.set_message("Saving Root Cause Commits");

    let (tx, rx) = channel::<
        env::AlteredCommit,
    >();

    let mut csv_wrt = csv::WriterBuilder::new().delimiter(b';').from_path(format!("{}/{}", prefix, filename)).unwrap();
    res.into_par_iter().for_each(|v| {
        let record = env::AlteredCommit{
            origin: v.0.to_string(),
            snapshot_src: v.1.to_string(),
            branch_name: v.2.to_string(),
            missing_commit: v.3.to_string(),
            snapshot_dst: v.4.to_string(),
            first_difference: None,
            main_category: None,
            sub_categories: None,
        };
        let tx = tx.clone();
        tx.send(record).expect(format!("Failed sending the msg for altered commit {}", v.3).as_str());
    });
    for _ in 0..res.len(){
        if let Ok(package) = rx.recv(){
            csv_wrt.serialize(package).expect(format!("couldn't write datas in file: {}", filename).as_str());
            bar.inc(1);
        }
    }
    bar.finish_with_message("Done Saving");
    progress.remove(&bar);
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

    let entries = fs::read_dir(prefix).expect("can't read dir");
    let multi_bar = MultiProgress::new();
    let bar_file = multi_bar.add(ProgressBar::new((entries.count() - 1) as u64));
    //let bar = ProgressBar::new((entries.count() - 1) as u64);
    let bar_style = ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap();
    bar_file.set_style(bar_style);
    bar_file.set_message("Amount of File");
    let count_file = AtomicUsize::new(0);

    //pool.install(|| {
        //rayon::scope(|thread| {
            fs::read_dir(prefix)
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
                        if env::RE_CSV.is_match(&filename) {
                            let name = &env::RE_FILENAME_WITHOUT_EXT
                                .captures(filename)
                                .expect("couldn't etract name of the csv file")[1];
                            let filename_dst = format!("{}/focus/{}_focus.csv", prefix, name);
                            match fs::metadata(&filename_dst) {
                                Ok(_) => warn!("analysis.rs > focus_missing_commits_all_files_with_save | File {} already exists", filename), //file already exists -> do nothing
                                Err(_) => {
                                    
                                    focus_missing_commits(
                                        &load_file_results(&opts, &filename).unwrap(),
                                        &graph,
                                        Some((opts, &filename)),
                                        &multi_bar,
                                    );
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
    bar_file.finish_with_message("Finished All Files");
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
    let mut cpt = 0;

    csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(b';')
        .from_path(format!("{prefix}/{filename}"))
        .unwrap()
        .deserialize()
        .into_iter()
        .for_each(|result: Result<env::AlteredCommit, csv::Error>|{
            cpt += 1;
            if let Ok(record)= result{
                res.insert((
                    record.origin,
                    record.snapshot_src,
                    record.branch_name,
                    record.missing_commit,
                    record.snapshot_dst,
                ));
            }else{
                info!("not the right amount of column: {cpt}");
            }
        });
    return Some(res);
}
