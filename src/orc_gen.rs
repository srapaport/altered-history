use crate::env;
use arrow_array::StringArray;
use arrow_schema::{DataType, Field, Schema};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use rand::{prelude::*, rng};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct AlteredCommitBis {
    pub origin: String,
    pub snapshot_src: String,
    pub branch_name: String,
    pub missing_commit: String,
    pub snapshot_dst: String,
    pub first_difference: Option<String>,
    pub main_category: env::MainCateg,
    pub sub_categories: Vec<env::SubCateg>,
}

pub fn csv_to_orc(_csv_path: impl AsRef<Path>, _orc_path: impl AsRef<Path>) {
    let _file = File::create("./test.orc").unwrap();
    let schema = Schema::new(vec![
        Field::new("origin", DataType::Utf8, false),
        Field::new("snapshot_src", DataType::Utf8, false),
        Field::new("branch_name", DataType::Utf8, false),
        Field::new("missing_commit", DataType::Utf8, false),
        Field::new("snapshot_dst", DataType::Utf8, false),
        Field::new("first_difference", DataType::Utf8, true),
        Field::new("main_category", DataType::Utf8, false),
        Field::new("sub_categories", DataType::List(Arc::new(Field::new("sub_category", DataType::Utf8, false))), false),
    ]);
    //let builder = orc_rust::ArrowWriterBuilder::new(file);
    let ac: Vec<AlteredCommitBis> = (0..2).into_iter().map(|_|{
        generate_random_altered_commit()
    }).collect();
    let mut origins = vec![];
    let mut mains = vec![];
    let mut fd = vec![];
    ac.into_iter().for_each(|ac|{
        origins.push(ac.origin);
        mains.push(ac.main_category.to_string());
        fd.push(ac.first_difference);
    });
    let origins = StringArray::from(origins);
    let fd = StringArray::from(fd);
    todo!("https://docs.rs/arrow-array/53.2.0/arrow_array/array/type.StringArray.html");


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
    (0..len)
        .map(|_| *chars.choose(&mut rng).unwrap())
        .collect()
}

fn random_main_category() -> env::MainCateg {
    let categories = vec![
        env::MainCateg::META,
        env::MainCateg::DIR,
        env::MainCateg::LoadingIssue
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
        env::SubCateg::Other
    ];
    let mut rng = rng();
    let num_categories = rng.random_range(1..=3);
    sub_categories
        .choose_multiple(&mut rng, num_categories)
        .cloned()
        .collect()
}

pub fn generate_random_altered_commit() -> AlteredCommitBis {
    let mut rng = rng();
    
    AlteredCommitBis {
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
        main_category: random_main_category(),
        sub_categories: random_sub_categories(),
    }
}