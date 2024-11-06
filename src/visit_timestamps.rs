use ar_row::deserialize::ArRowDeserialize;
use ar_row_derive::ArRowDeserialize;
use chashmap::CHashMap;
use indicatif::{HumanCount, ProgressBar, ProgressStyle};
use orc_rust::projection::ProjectionMask;
use orc_rust::ArrowReaderBuilder;
use rayon::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::fs::File;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use log::{debug, info};

use crate::env;

// Define structure
#[derive(ArRowDeserialize, Clone, Default, Debug, PartialEq, Eq)]
struct Visit {
    origin: String,
    date: Option<ar_row::Timestamp>,
    status: Option<String>,
    snapshot: Option<String>,
}

type AllVisits = CHashMap<String, Mutex<HashMap<String, Vec<i64>>>>;

fn load_orc_files(opts: &env::Options) -> AllVisits {
    let start = Instant::now();
    let files = std::fs::read_dir(&opts.orc_dir)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let all_visits = AllVisits::default();
    let expected_origins = opts.expected_origins as u64;
    let bar = ProgressBar::new(expected_origins);
    bar.set_style(ProgressStyle::with_template("{wide_bar} {eta}").unwrap());
    println!(
        "Pre-reserving memory for up to {} origins",
        HumanCount(expected_origins)
    );
    all_visits.reserve(expected_origins as usize);
    println!("Pre-reservation time: {:.2?}", start.elapsed());
    println!("Loading ORC files");
    let start = Instant::now();
    let count_snapshots = AtomicUsize::new(0);
    let count_visits = AtomicUsize::new(0);
    files.into_par_iter().for_each(|dir_entry| {
        let orc_path = dir_entry.path();
        info!(
            "orc_path : {}",
            orc_path
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap()
        );
        debug!("Extension orc OK");
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
    all_visits
}

fn save_db(all_visits: AllVisits, opts: &env::Options) {
    let start = Instant::now();
    println!("Saving database");
    let mut options = rocksdb::Options::default();
    options.create_if_missing(true);
    // TODO: do fine tuning on rocksdb options
    let db = rocksdb::DB::open(&options, &opts.output_dir).unwrap();
    let bar = ProgressBar::new(all_visits.len() as u64);
    bar.set_style(ProgressStyle::with_template("{wide_bar} {eta}").unwrap());
    for (origin, snapshots) in all_visits {
        let snapshots = snapshots.into_inner().unwrap();
        db.put(origin, bincode::serialize(&snapshots).unwrap())
            .unwrap();
        // Do not deallocate the hashmap and vectors
        std::mem::forget(snapshots);
        bar.inc(1);
    }
    bar.finish_and_clear();
    println!("DB writing time: {:.2?}", start.elapsed());
}

pub fn retrieve_visit_timestamps(opts: &env::Options) {
    stderrlog::new()
        .verbosity(2)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();
    let start = Instant::now();
    save_db(load_orc_files(opts), &opts);
    println!("Total time: {:.2?}", start.elapsed());
}
