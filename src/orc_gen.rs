use crate::env;
use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use csv;
use orc_rust::ArrowWriterBuilder;
use rand::{prelude::*, rng};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub fn csv_to_orc(csv_path: impl AsRef<Path>, orc_path: impl AsRef<Path>) {
    let file = File::create(orc_path).unwrap();
    let schema = Schema::new(vec![
        Field::new("origin", DataType::Utf8, false),
        Field::new("snapshot_src", DataType::Utf8, false),
        Field::new("branch_name", DataType::Utf8, false),
        Field::new("missing_commit", DataType::Utf8, false),
        Field::new("snapshot_dst", DataType::Utf8, false),
        Field::new("first_difference", DataType::Utf8, true),
        Field::new("main_category", DataType::Utf8, false),
        Field::new("sub_categories", DataType::Utf8, true),
        // Field::new("sub_categories", DataType::List(Arc::new(Field::new("sub_category", DataType::Utf8, false))), false),
    ]);

    //let builder = orc_rust::ArrowWriterBuilder::new(file);
    let ac: Vec<env::AlteredCommit> = (0..4)
        .into_iter()
        .map(|_| generate_random_altered_commit())
        .collect();
    let mut csv_wrt = csv::Writer::from_path(csv_path).unwrap();
    ac.clone().into_iter().for_each(|com| {
        csv_wrt.serialize(com).unwrap();
    });
    csv_wrt.flush().unwrap();

    let mut origins = vec![];
    let mut snapshot_src = vec![];
    let mut branch_name = vec![];
    let mut missing_commit = vec![];
    let mut snapshot_dst = vec![];
    let mut fd = vec![];
    let mut mains = vec![];
    let mut sub = vec![];

    // let mut sub_data = vec![];
    // let mut offsets: Vec<i32> = vec![0];
    // let mut total_len: i32 = 0;

    ac.into_iter().for_each(|ac| {
        origins.push(ac.origin);
        fd.push(ac.first_difference);
        mains.push(ac.main_category.unwrap().to_string());
        snapshot_src.push(ac.snapshot_src);
        branch_name.push(ac.branch_name);
        missing_commit.push(ac.missing_commit);
        snapshot_dst.push(ac.snapshot_dst);
        sub.push(ac.sub_categories.unwrap());

        // let sub_strings: Vec<String> = ac.sub_categories;
        // total_len += sub_strings.len() as i32;
        // offsets.push(total_len);
        // sub_data.extend(sub_strings);
    });
    let origins = StringArray::from(origins);
    let snapshot_src = StringArray::from(snapshot_src);
    let branch_name = StringArray::from(branch_name);
    let missing_commit = StringArray::from(missing_commit);
    let snapshot_dst = StringArray::from(snapshot_dst);
    let fd = StringArray::from(fd);
    let mains = StringArray::from(mains);
    let sub = StringArray::from(sub);

    // let sub_array = StringArray::from(sub_data);
    // let sub = arrow_array::ListArray::try_new(
    //     Arc::new(Field::new("sub_categories", DataType::List(Arc::new(Field::new("sub_category", DataType::Utf8, false))), false)),
    //     arrow_buffer::buffer::OffsetBuffer::new(arrow_buffer::buffer::ScalarBuffer::from(offsets)),
    //     Arc::new(sub_array),
    //     None,
    // ).unwrap();

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(origins),
            Arc::new(snapshot_src),
            Arc::new(branch_name),
            Arc::new(missing_commit),
            Arc::new(snapshot_dst),
            Arc::new(fd),
            Arc::new(mains),
            Arc::new(sub),
        ],
    )
    .expect("couldn't build batch");
    let mut writer = ArrowWriterBuilder::new(file, batch.schema())
        .try_build()
        .unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

/*
let file = File::create("/path/to/file.orc").unwrap();
let batch = get_record_batch();
let mut writer = ArrowWriterBuilder::new(file, batch.schema())
    .try_build()
    .unwrap();
writer.write(&batch).unwrap();
writer.close().unwrap();
*/
fn generate_random_string(len: usize) -> String {
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut rng = rng();
    (0..len).map(|_| *chars.choose(&mut rng).unwrap()).collect()
}

fn random_main_category() -> env::MainCateg {
    let categories = vec![
        env::MainCateg::META,
        env::MainCateg::DIR,
        env::MainCateg::LoadingIssue,
    ];
    *categories.choose(&mut rng()).unwrap()
}

fn random_sub_categories() -> Vec<env::SubCateg> {
    let sub_categories = vec![
        env::SubCateg::Author,
        env::SubCateg::Date,
        env::SubCateg::Message,
        env::SubCateg::Committer,
        env::SubCateg::CommitterDate,
        env::SubCateg::DifferentBranchName,
        env::SubCateg::RemovedBranch,
        env::SubCateg::FileModified,
        env::SubCateg::FileRemoved,
        env::SubCateg::ContentSplit,
        env::SubCateg::Other,
    ];
    let mut rng = rng();
    let num_categories = rng.random_range(1..=3);
    sub_categories
        .choose_multiple(&mut rng, num_categories)
        .cloned()
        .collect()
}

pub fn generate_random_altered_commit() -> env::AlteredCommit {
    let mut rng = rng();

    env::AlteredCommit {
        origin: format!("http/{}", generate_random_string(8)),
        snapshot_src: format!("snap-{}", generate_random_string(6)),
        branch_name: format!("branch-{}", generate_random_string(4)),
        missing_commit: generate_random_string(10),
        snapshot_dst: format!("snap-{}", generate_random_string(6)),
        first_difference: if rng.random_bool(0.8) {
            Some(generate_random_string(8))
        } else {
            None
        },
        main_category: Some(random_main_category()),
        sub_categories: Some(
            random_sub_categories()
                .into_iter()
                .map(|cat| cat.to_string())
                .reduce(|a, b| format!("{},{}", a, b))
                .unwrap(),
        ),
    }
}
