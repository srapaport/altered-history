pub mod env;
pub mod analysis;
pub mod utils;
pub mod classes;
pub mod visit_timestamps;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::ffi::OsString;
use std::io::{prelude::*, BufReader};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::cell::Cell;
use std::thread;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::path::PathBuf;
use ar_row::arrow::array::RecordBatch;
use chashmap::CHashMap;
use regex::Regex;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use rocksdb::{DB, Options};
use indicatif::{HumanCount, ProgressBar, ProgressStyle};
use swh_graph::graph::*;
use swh_graph::labels::EdgeLabel;
use swh_graph::NodeType;
use swh_graph::java_compat::mph::gov::GOVMPH;
use orc_rust::projection::ProjectionMask;
use orc_rust::ArrowReaderBuilder;
use ar_row::deserialize::ArRowDeserialize;
use ar_row_derive::ArRowDeserialize;
use log::{debug, info, warn};

extern "C" {
    fn gettid() -> u32;
}

pub fn snapshots_sorting(snapshots: Box<[u8]>) -> VecDeque<String>{
    // We are sorting the snapshot by their minimum timestamp
    let mut sorted_snapshots = Vec::new();
    let decoded_value: env::Visits = bincode::deserialize(&snapshots[..]).unwrap();
    for (snapshot, ts) in decoded_value.snapshots.iter(){
        //debug!("    snapshot id: {}", snapshot);
        //debug!("        timestamps: {:?}", ts);
        if snapshot == env::EMPTY_SNAPSHOT{
            continue;
        }
        sorted_snapshots.push((snapshot.clone(), *ts.iter().min().unwrap()));
    }
    sorted_snapshots.sort_by(|a, b| a.1.cmp(&b.1));
    //debug!("tuple sorted snaphosts: {:?}", sorted_snapshots);
    sorted_snapshots.into_iter().map(|(snap, _)| snap).collect()
}

pub fn missing_commits_v1<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snapshots: &mut VecDeque<String>,
    graph: &G,
) -> Option<HashSet<(String, String, String, String)>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    
    let mut curr_missing_commits : HashSet<(String, String, String, String)> = HashSet::new();
    
    // We want to check if a commit is missing from a snapshot to another so we need at least 2 commits
    if snapshots.len() < 2{
        return None;
    }

    let mut swhid1 = "swh:1:snp:".to_string() + &(snapshots.pop_front().unwrap());
    //debug!("");
    //debug!("New Snapshot");
    let mut set_branch_kept = HashSet::new();
    let mut set_branch_rejected = HashSet::new();
    let mut swhid1_commits = find_all_commits_v1(&swhid1, &mut set_branch_kept, &mut set_branch_rejected, graph).unwrap();

    'find_missing_commits: loop{
        let swhid2 = "swh:1:snp:".to_string() + &(snapshots.pop_front().unwrap());
        //debug!("");
        //debug!("New Snapshot");
        let swhid2_commits = find_all_commits_v1(&swhid2, &mut set_branch_kept, &mut set_branch_rejected, graph).unwrap();

        let branch_missing_commits = compare_maps(&swhid1_commits, &swhid2_commits);
        if branch_missing_commits.len() > 0 {
            curr_missing_commits.extend(
                branch_missing_commits
                .into_iter()
                .map(|(branch_name, commit)| -> (String, String, String, String) {(swhid1.clone(), branch_name, commit, swhid2.clone())})
                .collect::<HashSet<(String, String, String, String)>>()
            );
        }
        else{
            //debug!("No missing commits yet");
        }

        //break 'find_missing_commits;
        swhid1 = swhid2;
        swhid1_commits = swhid2_commits;
        
        if snapshots.len() < 1{
            break 'find_missing_commits;
        }
    }
    env::BRANCH_KEPT.fetch_add(set_branch_kept.len(), Ordering::Relaxed);
    env::BRANCH_REJECTED.fetch_add(set_branch_rejected.len(), Ordering::Relaxed);
    //debug!("Current missing commits: {:?}", curr_missing_commits);
    if curr_missing_commits.len() == 0{
        return None;
    }
    Some(curr_missing_commits)
}

pub fn find_all_commits_v1<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    snap_swhid: &String, 
    set_branch_kept: &mut HashSet<String>,
    set_branch_rejected: &mut HashSet<String>,
    graph: &G
) -> Option<HashMap<String, HashSet<String>>>
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut all_commits: HashMap<String, HashSet<String>> = HashMap::new();
    //Check that the swhid given is a snapshot
    if !env::RE_SNP.is_match(&snap_swhid){
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
        if !(succ_type == NodeType::Revision) && !((succ_type == NodeType::Release)){
            debug!("A snapshot successor is neither a release nor a revision. Snapshot: {} | Successor: {}", snap_swhid, succ_swhid);
            continue;
        }

        //We gather all the branches associated to this successor
        let mut branch_list: Vec<String> = Vec::new();
        for label in labels{
            let branch_name: String;
            //let filename = graph.properties().label_name(label);
            if let EdgeLabel::Branch(branch) = label{
                branch_name = String::from_utf8_lossy(
                    &(graph.properties().label_name(branch.filename_id()))
                ).to_string();
                if !env::RE_BRANCH.is_match(&branch_name){
                    set_branch_rejected.insert(branch_name);
                    continue;
                }
                set_branch_kept.insert(branch_name.clone());
            }
            else{
                continue;
            }
            /* let filename = graph.properties().label_name(label.filename_id());
            let branch_name = String::from_utf8_lossy(&filename).to_string();
            if !env::RE_BRANCH.is_match(&branch_name){
                continue;
            } */
            branch_list.push(branch_name);
        }

        if branch_list.len() == 0 {
            continue;
        } 
        //info!("branch list: {:?}", branch_list);
        //info!("branch list size: {}", branch_list.len());
        //We gather all the successor of this successor in this branch
        find_all_commits_aux_v1(&mut all_commits, &branch_list, succ, graph);
    }
    Some(all_commits)
}

fn find_all_commits_aux_v1<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    all_commits: &mut HashMap<String, HashSet<String>>,
    branch_list: &Vec<String>,
    node: usize,
    graph: &G
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
{
    let mut nodes_to_visit = Vec::new();
    nodes_to_visit.push(node);
    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(node) = nodes_to_visit.pop(){
        //debug!("size nodes_to_visit: {}", nodes_to_visit.len());
        if visited.contains(&node){
            continue;
        }
        visited.insert(node);
        let successors = graph.successors(node);
        for succ in successors{
            let succ_swhid = graph.properties().swhid(succ).to_string();
            if !env::RE_REV.is_match(&succ_swhid) && !env::RE_REL.is_match(&succ_swhid){
                continue;
            }
            for branch in branch_list{
                all_commits
                    .entry(branch.clone())
                    .or_insert_with(HashSet::new)
                    .insert(succ_swhid.clone());
            }
            nodes_to_visit.push(succ);
        }
    }
    
}

pub fn compare_maps(is_included: &HashMap<String, HashSet<String>>, include: &HashMap<String, HashSet<String>>) -> HashSet<(String, String)>{
    let mut not_included: HashSet<(String, String)> = HashSet::new();

    /////////printing
    //debug!("Is included:");
    //map_pp(is_included);
    //debug!("");
    //debug!("Include:");
    //map_pp(include);
    /////////
    
    //debug!("/////////// Comparing Maps ///////////");

    for (branch_name, commits_included) in is_included{
        match include.get(branch_name){
            Some(include_commits) => {
                for commit_included in commits_included{
                    if include_commits.contains(commit_included){
                        continue;
                    }
                    not_included.insert((branch_name.clone(), commit_included.clone()));
                }
            },

            None => {
                //debug!("Branch: {}, couldn't be found", branch_name);
                continue},
        }
    }
    //debug!("Not included: {:?}", not_included);
    not_included
}

pub fn main_all_database_mpsc(opts: &env::Options){
    let basename: &str;
    let db_path: String;
    let number_of_origins: usize;
    let prefix_res: &str;
    let chunk: usize;
    match opts.dataset.as_str(){
        "2021" => {
            basename = env::BASENAME_2021;
            db_path = "/infres/ir800/rapaport/phd-solal/dev/rust/datasets/2021".to_string();
            number_of_origins = env::ORIGINS_2021;
            prefix_res = env::PREFIX_RESULTS_2021;
            chunk = 10;
        },
        "2023" => todo!(),
        "FULL" => {
            basename = env::BASENAME_FULL;
            db_path = format!("{}/db_origin_visit",env::DATABASE_FULL);
            number_of_origins = env::ORIGINS_FULL;
            prefix_res = env::PREFIX_RESULTS_FULL;
            chunk = 10_000;
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }

    let (tx, rx) = channel::<(String, Option<HashSet<(String, String, String, String)>>)>();
    
    thread::spawn(move ||{
        let workers = num_cpus::get()/3;

        let graph = SwhUnidirectionalGraph::new(PathBuf::from(basename))
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");
    
        let mut options = Options::default();
        let db = DB::open(&options, db_path).unwrap();
        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap());
        let start = Instant::now();

        pool.install(||{
            rayon::scope(|thread| {
                for result in db.iterator(rocksdb::IteratorMode::Start){
                    match result{
                        Ok((key, value)) => {
                            thread.spawn({ let tx = tx.clone(); |_|{
                                let tx = tx;
                                let key = key;
                                let value = value;
                                count_origins.fetch_add(1, Ordering::Relaxed);
                                let key_str = String::from_utf8(key.to_vec()).unwrap();
                                let thread_id = unsafe { gettid() };
                                info!("Checking Origin: {} | with pid: {:?}", key_str, thread_id);
            
                                let mut sorted_snapshots = snapshots_sorting(value);
                                debug!("    sorted snapshots: {:?}", sorted_snapshots);
            
                                ///////////// missing_commits
                                if let Some(curr_missing_commits) = missing_commits_v1(&mut sorted_snapshots, &graph){
                                    info!("Missing commits in {}", key_str);
                                    tx.send((key_str, Some(curr_missing_commits))).expect("Failed sending the msg");
                                }
                                else{
                                    tx.send((key_str, None)).expect("Failed sending the msg");
                                }
                                let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                                bar.set_position(curr_nb_origins);
                            }});
                        },
                        Err(_) => continue,
                    }
                }
            });
        });
        bar.finish_and_clear();
        //println!("Time elapsed: {:.2?}", start.elapsed());
    });

    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    File::create(format!("{}/checkpoints", prefix_res)).expect("couldn't create checkpoint file");

    for _ in 0..number_of_origins{
        let (ori, value) = rx.recv().expect("Couldn't receive data");
        if let Some(mc) = value{
            if res.len() > chunk{
                flush_map(prefix_res, &mut res);
                create_cp(&opts, &mut cp);
            }
            res.insert(ori.clone(), mc);
        }
        cp.insert(ori);
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0{
        create_cp(&opts, &mut cp);
    }
    
}

pub fn main_all_database_mpsc_with_cp(opts: &env::Options, checkpoint: HashSet<String>){
    let basename: &str;
    let db_path: String;
    let number_of_origins: usize;
    let prefix_res: &str;
    let chunk: usize;
    match opts.dataset.as_str(){
        "2021" => {
            basename = env::BASENAME_2021;
            db_path = "/infres/ir800/rapaport/phd-solal/dev/rust/datasets/2021".to_string();
            number_of_origins = env::ORIGINS_2021;
            prefix_res = env::PREFIX_RESULTS_2021;
            chunk = 10;
        },
        "2023" => todo!(),
        "FULL" => {
            basename = env::BASENAME_FULL;
            db_path = format!("{}/db_origin_visit",env::DATABASE_FULL);
            number_of_origins = env::ORIGINS_FULL;
            prefix_res = env::PREFIX_RESULTS_FULL;
            chunk = 10_000;
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }

    let (tx, rx) = channel::<Option<(String, Option<HashSet<(String, String, String, String)>>)>>();
    
    thread::spawn(move ||{
        let workers = num_cpus::get()/3;

        let graph = SwhUnidirectionalGraph::new(PathBuf::from(basename))
            .expect("Could not load graph")
            .init_properties()
            .load_properties(|properties| properties.load_maps::<GOVMPH>())
            .expect("Could not load maps")
            .load_properties(|properties| properties.load_label_names())
            .expect("Could no load label names")
            .load_labels()
            .expect("Could not load labels");
    
        let mut options = Options::default();
        let db = DB::open(&options, db_path).unwrap();
        let count_origins = AtomicUsize::new(0);
        let pool = ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
        let bar = ProgressBar::new(number_of_origins as u64);
        bar.set_style(ProgressStyle::with_template("{wide_bar} {pos} {percent_precise}% {elapsed_precise} {duration_precise} {eta}").unwrap());
        let start = Instant::now();

        pool.install(||{
            rayon::scope(|thread| {
                for result in db.iterator(rocksdb::IteratorMode::Start){
                    match result{
                        Ok((key, value)) => {
                            thread.spawn({ let tx = tx.clone(); |_|{
                                let tx = tx;
                                let key = key;
                                let value = value;
                                count_origins.fetch_add(1, Ordering::Relaxed);
                                let key_str = String::from_utf8(key.to_vec()).unwrap();
                                ///////// Skipping if necessary according to the checkpoints
                                if !checkpoint.contains(&key_str){
                                    let thread_id = unsafe { gettid() };
                                    info!("Checking Origin: {} | with pid: {:?}", key_str, thread_id);
                                    let mut sorted_snapshots = snapshots_sorting(value);
                                    debug!("    sorted snapshots: {:?}", sorted_snapshots);
                
                                    ///////////// missing_commits
                                    if let Some(curr_missing_commits) = missing_commits_v1(&mut sorted_snapshots, &graph){
                                        info!("Missing commits in {} | with pid: {}", key_str, std::process::id());
                                        tx.send(Some((key_str, Some(curr_missing_commits)))).expect("Failed sending the msg");
                                    }
                                    else{
                                        tx.send(Some((key_str, None))).expect("Failed sending the msg");
                                    }
                                    let curr_nb_origins = count_origins.load(Ordering::Relaxed) as u64;
                                    bar.set_position(curr_nb_origins);
                                }
                                else{
                                    info!("Not calculating Origin: {}", key_str);
                                    tx.send(None).expect("Failed sending the msg");
                                }
                            }});
                        },
                        Err(_) => continue,
                    }
                }
            });
        });
        bar.finish_and_clear();
        //println!("Time elapsed: {:.2?}", start.elapsed());
    });

    let mut res: HashMap<String, HashSet<(String, String, String, String)>> = HashMap::new();
    let mut cp: HashSet<String> = HashSet::new();
    for _ in 0..number_of_origins{
        if let Some(package) = rx.recv().expect("Couldn't receive data"){
            let (ori, value) = package;
            if let Some(mc) = value{
                if res.len() > chunk{
                    flush_map(prefix_res, &mut res);
                    create_cp(&opts, &mut cp);
                }
                res.insert(ori.clone(), mc);
            }
            cp.insert(ori);
        }
    }
    if res.len() > 0 {
        flush_map(prefix_res, &mut res);
    }
    if cp.len() > 0{
        create_cp(&opts, &mut cp);
    }
    
}

pub fn main_targeted_origin(opts: &env::Options, ori: &str) -> Option<HashSet<(String, String, String, String)>>{
    let basename: &str;
    let db_path: String;
    match opts.dataset.as_str(){
        "2021" => {
            basename = env::BASENAME_2021;
            db_path = "/infres/ir800/rapaport/phd-solal/dev/rust/datasets/2021".to_string();
        },
        "2023" => todo!(),
        "FULL" => {
            basename = env::BASENAME_FULL;
            db_path = format!("{}/db_origin_visit",env::DATABASE_FULL);
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL> <Number of workers>");
            unimplemented!()
        },
    }
    let graph = load_unidirectional(PathBuf::from(basename))
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
    let start = Instant::now();

    let value: Box<[u8]> = db.get(ori).unwrap().unwrap().into();

    info!("Origin: {}", ori);

    let mut sorted_snapshots = snapshots_sorting(value);
    debug!("    sorted snapshots: {:?}", sorted_snapshots);

    ///////////// missing_commits
    let Some(curr_missing_commits) = missing_commits_v1(&mut sorted_snapshots, &graph)
    else{
        info!("No missing commit in {}", ori);
        return None
    };
    //info!("Time elapsed: {:.2?}", start.elapsed());
    Some(curr_missing_commits)
}

pub fn flush_map(prefix: &str, map: &mut HashMap<String, HashSet<(String, String, String, String)>>){
    let swhid = format!("{}",map.iter().next().unwrap().1.iter().next().unwrap().0);
    let filename = format!("{}/{}.csv", prefix, swhid);
    let filename_tmp = format!("{}/{}.tmp", prefix, swhid);
    let Ok(mut file_w) = File::create_new(format!("{}", filename_tmp))
        else {
            warn!("File {} already exists!", filename_tmp);
            return
        };
    file_w
        .write("origin;snapshot_src;branch_name;missing_commit;snapshot_dst\n".as_bytes())
        .expect(format!("couldn't write headings in file: {}", filename_tmp).as_str());
    map.iter().for_each(|v|{
        v.1.iter().for_each(|set|{
            file_w
                .write(format!("{};{};{};{};{}\n", v.0, set.0, set.1, set.2, set.3).as_bytes())
                .expect(format!("couldn't write datas in file: {}", filename_tmp).as_str());
        });
    });
    map.clear();
    drop(file_w); // didn't end with this instruction one time
    std::fs::rename(&filename_tmp, &filename).expect(format!("Couldn't rename {} into {}", filename_tmp, filename).as_str());
}

pub fn load_results(opts: &env::Options) -> Option<HashSet<String>>{
    let prefix: &str;
    match opts.dataset.as_str(){
        "2021" => prefix = env::PREFIX_RESULTS_2021,
        "2023" => todo!(),
        "FULL" => prefix = env::PREFIX_RESULTS_FULL,
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }
    let start = Instant::now();
    let mut res = HashSet::new();

    fs::read_dir(prefix)
        .expect(format!("couldn't find dir {}", prefix).as_str())
        .into_iter()
        .for_each(|file|{
            let filename = file.unwrap().file_name().into_string().unwrap();
            let re_csv = Regex::new(r".*\.csv$").unwrap();
            if re_csv.is_match(&filename){
                info!("loading res from file: {filename}");
                csv::ReaderBuilder::new()
                    .delimiter(b';')
                    .from_path(format!("{prefix}/{filename}"))
                    .unwrap()
                    .records()
                    .into_iter()
                    .for_each(|result|{
                        res.insert(result.unwrap().get(0).unwrap().to_string());
                    });
            }
            else{
                info!("not loading tmp file: {filename}");
            }
        });
        
    //info!("Time elapsed: {:.2?}", start.elapsed());
    return Some(res);
}

pub fn create_cp(opts: &env::Options, cp: &mut HashSet<String>){
    let prefix: &str;
    match opts.dataset.as_str(){
        "2021"=> prefix = env::PREFIX_RESULTS_2021,
        "2023"=> prefix = env::PREFIX_RESULTS_2023,
        "FULL"=> prefix = env::PREFIX_RESULTS_FULL,
        _ => unimplemented!(),
    }

    let mut cp_file = fs::OpenOptions::new()
        .append(true)
        .open(format!("{}/checkpoints", prefix).as_str())
        .expect(format!("cannot open checkpoints file").as_str());
    cp.iter().for_each(|ori|{
        cp_file
            .write(format!("{}\n", ori).as_bytes())
            .expect("couldn't write data in checkpoints");
    });
    cp.clear();
}

pub fn load_checkpoints(opts: &env::Options) -> Option<HashSet<String>>{
    let prefix: &str;
    match opts.dataset.as_str(){
        "2021" => prefix = env::PREFIX_RESULTS_2021,
        "2023" => todo!(),
        "FULL" => prefix = env::PREFIX_RESULTS_FULL,
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }
    let mut res = HashSet::new();

    if let Ok(cp_file) = File::open(format!("{}/checkpoints", prefix)){
        BufReader::new(cp_file)
            .lines()
            .into_iter()
            .for_each(|line|{
                if let Ok(ori) = line{
                    res.insert(ori);
                }
            });
    }
    else{
        info!("couldn't open checkpoints file");
        return None;
    }

    return Some(res);
}

#[derive(ArRowDeserialize, Clone, Default, Debug, PartialEq, Eq)]
struct Visit {
    origin: String,
    date: Option<ar_row::Timestamp>,
    status: Option<String>,
    snapshot: Option<String>,
}

pub type AllVisits = CHashMap<String, Mutex<HashMap<String, Vec<i64>>>>;

pub fn load_orc_files(opts: &env::Options) -> Option<AllVisits> {
    let orc_dir: &str;
    let expected_origins: usize;
    match opts.dataset.as_str(){
        "2021" =>{
            //orc_dir = format!("{}/origin_visit_status", env::DATABASE_2021).as_str();
            orc_dir = env::DATABASE_2021;
            expected_origins = env::ORIGINS_2021;
        },
        "2023" =>{
            //orc_dir = format!("{}/origin_visit_status", env::DATABASE_2023).as_str();
            //expected_origins = env::ORIGINS_2023;
            todo!()//Don't know the number of origins
        },
        "FULL" =>{
            //orc_dir = format!("{}/origin_visit_status", env::DATABASE_FULL).as_str();
            orc_dir = env::DATABASE_FULL;
            expected_origins = env::ORIGINS_FULL;
        },
        _ => {
            warn!("Usage: <2021, 2023, FULL>");
            unimplemented!()
        },
    }
    let start = Instant::now();
    let files = std::fs::read_dir(format!("{}/origin_visit_status", orc_dir))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let all_visits = AllVisits::default();
    //let expected_origins = expected_origins as u64 * 1_000_000;
    let bar = ProgressBar::new(expected_origins as u64);
    bar.set_style(ProgressStyle::with_template("{wide_bar} {eta}").unwrap());
    println!("Pre-reserving memory for up to {} origins", HumanCount(expected_origins as u64));
    all_visits.reserve(expected_origins);
    println!("Pre-reservation time: {:.2?}", start.elapsed());
    println!("Loading ORC files");
    let start = Instant::now();
    let count_snapshots = AtomicUsize::new(0);
    let count_visits = AtomicUsize::new(0);
    files.into_par_iter().for_each(|dir_entry| {
        info!("orc_path : {}", dir_entry.path().file_name().unwrap().to_os_string().into_string().unwrap());
        let file = File::open(dir_entry.path()).expect("could not open .orc");
        let builder = ArrowReaderBuilder::try_new(file).expect("could not make builder");
        let projection = ProjectionMask::named_roots(
            builder.file_metadata().root_data_type(),
            &["origin", "date", "status", "snapshot"],
        );
        let reader = builder.with_projection(projection).build();
        reader.for_each(|batch| {
            debug!("Entering reader");
            let batch = batch.unwrap();
            let snapshots = AtomicUsize::new(0);
            let mut visits = 0;
            for visit in <Visit>::from_record_batch(batch).unwrap() {
                if visit.status.as_deref() == Some("full") {
                    let origin = visit.origin;
                    let date = visit.date.unwrap().seconds;
                    if visit.snapshot.is_none() {
                        continue;
                    }
                    let snapshot = Cell::new(visit.snapshot);
                    visits += 1;
                    all_visits.upsert(
                        origin,
                        || {
                            let mut h = HashMap::new();
                            h.insert(snapshot.take().unwrap(), vec![date]);
                            snapshots.fetch_add(1, Ordering::Relaxed);
                            Mutex::new(h)
                        },
                        |v| {
                            let snapshot = snapshot.take().unwrap();
                            v.get_mut()
                                .unwrap()
                                .entry(snapshot)
                                .or_insert_with(|| {
                                    snapshots.fetch_add(1, Ordering::Relaxed);
                                    Vec::new()
                                })
                                .push(date);
                        },
                    );
                }
            }
            count_visits.fetch_add(visits, Ordering::Relaxed);
            count_snapshots.fetch_add(snapshots.load(Ordering::Relaxed), Ordering::Relaxed);
            bar.set_position(all_visits.len() as u64);
        });
    });
    bar.finish_and_clear();
    println!(
        "#origins = {} ‑ #snapshots = {} ‑ #visits = {} ‑ loading time: {:.2?}",
        HumanCount(all_visits.len() as u64),
        HumanCount(count_snapshots.load(Ordering::Relaxed) as u64),
        HumanCount(count_visits.load(Ordering::Relaxed) as u64),
        start.elapsed()
    );
    return Some(all_visits);
}
