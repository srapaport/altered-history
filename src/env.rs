use clap::Parser;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::AtomicUsize;

#[derive(Parser)]
pub struct Options {
    /// Directory containing the orc files
    pub orc_dir: String,
    /// Directory containing the database files - no slash at the end
    pub output_dir: String,
    /// path of the compressed graph -> e.g. "./datasets/2021-03-23-popular-3k-python-graph/graph"
    pub graph: String,
    /// path where results are stored - no slash at the end - must exist -> e.g."./results/2023"
    pub results: String,
    /// amount of origins in the graph
    pub expected_origins: usize,
    /// amount of origin with an altered history per result file
    pub chunk: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Visits {
    pub snapshots: HashMap<String, Vec<i64>>,
}

pub const ORIGINS_2021: usize = 2_181;
pub const ORIGINS_FULL: usize = 226_726_529;
pub const EMPTY_SNAPSHOT: &str = "1a8893e6a86f444e8be8e7bda6cb34fb1735a00e";
pub static RE_BRANCH: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^refs/heads/(main|master|dev|devel|develop|development)$").unwrap());
pub static RE_CSV: Lazy<Regex> = Lazy::new(|| Regex::new(r".*\.csv$").unwrap());
pub static RE_FILENAME_WITHOUT_EXT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.+)\..*$").unwrap());
pub const MAX_DEPTH: usize = 10;

pub static ORI_KEPT: AtomicUsize = AtomicUsize::new(0);
pub static ORI_REJECTED: AtomicUsize = AtomicUsize::new(0);
pub static BRANCHES_KEPT: AtomicUsize = AtomicUsize::new(0);
pub static BRANCHES_REJECTED: AtomicUsize = AtomicUsize::new(0);
pub static TOTAL_REV_ANALYZED: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum MainCateg {
    META,
    DIR,
    LoadingIssue,
}

impl fmt::Display for MainCateg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            MainCateg::META => write!(f, "META"),
            MainCateg::DIR => write!(f, "DIR"),
            MainCateg::LoadingIssue => write!(f, "LoadingIssue"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum SubCateg {
    Message,
    Author,
    Date,
    Committer,
    CommitterDate,
    DifferentBranchName,
    RemovedBranch, //TODO
    FileModified,
    FileRemoved,
    ContentSplit,
}

impl fmt::Display for SubCateg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SubCateg::Message => write!(f, "Message"),
            SubCateg::Author => write!(f, "Author"),
            SubCateg::Date => write!(f, "Date"),
            SubCateg::Committer => write!(f, "Committer"),
            SubCateg::CommitterDate => write!(f, "CommitterDate"),
            SubCateg::DifferentBranchName => write!(f, "DifferentBranchName"),
            SubCateg::RemovedBranch => write!(f, "RemovedBranch"), //TODO
            SubCateg::FileModified => write!(f, "FileModified"),
            SubCateg::FileRemoved => write!(f, "FileRemoved"),
            SubCateg::ContentSplit => write!(f, "ContentSplit"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Categ {
    pub main_categ: MainCateg,
    pub sub_categ: HashSet<SubCateg>, //if main_categ is META then None else Some(ext)
}
