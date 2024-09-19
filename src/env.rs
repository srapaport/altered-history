use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::fmt;
use chashmap::CHashMap;
use serde::{Serialize, Deserialize};
use regex::Regex;
use once_cell::sync::Lazy;
use clap::Parser;
use swh_graph::properties::*;
use swh_graph::SwhGraphProperties;
use swh_graph::graph::*;
use swh_graph::java_compat::mph::gov::GOVMPH;
/* use dsi_bitstream::prelude::BigEndian;
use webgraph::prelude::*;
use webgraph::graphs::BVGraph;
use webgraph::labels::swh_labels::{MmapReaderBuilder, SwhLabels};
use sux::dict::EliasFano;
use sux::rank_sel::SelectFixed2;
use sux::bits::CountBitVec;
use sux::bits::BitFieldVec; */
use ar_row::deserialize::ArRowDeserialize;
use ar_row_derive::ArRowDeserialize;

#[derive(Parser)]
pub struct Options {
    /// On what dataset is the algorithm working
    pub dataset: String,
}

#[derive(ArRowDeserialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct Visit {
    pub origin: String,
    pub date: Option<ar_row::Timestamp>,
    pub status: Option<String>,
    pub snapshot: Option<String>,
}

#[derive(ArRowDeserialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct Revision {
    pub id: String,
    pub message: Option<Box<[u8]>>,
    pub author: Option<Box<[u8]>>,
    pub date: Option<ar_row::Timestamp>,
    pub date_offset: Option<i16>,
    pub committer: Option<Box<[u8]>>,
    pub committer_date: Option<ar_row::Timestamp>,
    pub committer_offset: Option<i16>,
    pub directory: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Visits {
    pub snapshots: HashMap<String, Vec<i64>>,
}

pub type AllVisits = CHashMap<String, Mutex<HashMap<String, Vec<i64>>>>;

pub const BASENAME_2021: &str = "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph";
pub const BASENAME_2021_T: &str = "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-graph/graph-transposed";
pub const BASENAME_2023: &str = "/infres/ir800/rapaport/datasets/2023-09-06-popular-1k/compressed/graph";
pub const BASENAME_FULL: &str = "/infres/ir800/rapaport/datasets/2023-09-06/compressed/graph";
pub const DATABASE_2021: &str = "/infres/ir800/rapaport/datasets/2021-03-23-popular-3k-python-orc";
pub const DATABASE_2023: &str = "/infres/ir800/rapaport/datasets/2023-09-06-popular-1k";
pub const DATABASE_FULL: &str = "/infres/ir800/rapaport/datasets/2023-09-06";
pub const PREFIX_RESULTS_2021: &str = "./results/2021/new";
pub const PREFIX_RESULTS_2023: &str = "./results/2023";
pub const PREFIX_RESULTS_FULL: &str = "/infres/ir800/rapaport/results/FULL";
pub const ORIGINS_2021: usize = 2_181;
pub const ORIGINS_FULL: usize = 226_726_529;
pub const EMPTY_SNAPSHOT: &str = "1a8893e6a86f444e8be8e7bda6cb34fb1735a00e";
pub static RE_SNP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^swh:.:snp:.*$").unwrap());
pub static RE_REV: Lazy<Regex> = Lazy::new(|| Regex::new(r"^swh:.:rev:.*$").unwrap());
pub static RE_REL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^swh:.:rel:.*$").unwrap());
pub static RE_DIR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^swh:.:dir:.*$").unwrap());
pub static RE_DEV: Lazy<Regex> = Lazy::new(|| Regex::new(r".*dev.*").unwrap());
pub static RE_MAS: Lazy<Regex> = Lazy::new(|| Regex::new(r".*master.*").unwrap());
pub static RE_BRANCH: Lazy<Regex> = Lazy::new(|| Regex::new(r"refs\/heads\/(ma(in([^t])|ster)|dev(el(op)?)?)").unwrap());
pub static RE_CSV: Lazy<Regex> = Lazy::new(|| Regex::new(r".*\.csv$").unwrap());
pub static RE_FILENAME_WITHOUT_EXT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.+)\..*$").unwrap());
pub const MAX_DEPTH: usize = 10;


#[derive(Debug, PartialEq, Eq, Hash)]
pub enum MainCateg{
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
pub enum SubCateg{
    Message,
    Author,
    Date,
    //DateOffset,
    Committer,
    CommitterDate,
    //CommitterOffset,
    Directory,
    DifferentBranchName,
    RemovedBranch,
    ContentModified,
    ContentRemoved,
    ContentDiluted,
}

impl fmt::Display for SubCateg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SubCateg::Message => write!(f, "Message"),
            SubCateg::Author => write!(f, "Author"),
            SubCateg::Date => write!(f, "Date"),
            //SubCateg::DateOffset => write!(f, "DateOffset"),
            SubCateg::Committer => write!(f, "Committer"),
            SubCateg::CommitterDate => write!(f, "CommitterDate"),
            //SubCateg::CommitterOffset => write!(f, "CommitterOffset"),
            SubCateg::Directory => write!(f, "Directory"),
            SubCateg::DifferentBranchName => write!(f, "DifferentBranchName"),
            SubCateg::RemovedBranch => write!(f, "RemovedBranch"),
            SubCateg::ContentModified => write!(f, "ContentModified"),
            SubCateg::ContentRemoved => write!(f, "ContentRemoved"),
            SubCateg::ContentDiluted => write!(f, "ContentDiluted"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Categ{
    pub main_categ: MainCateg,
    pub sub_categ: HashSet<SubCateg>,//if main_categ is META then None else Some(ext)
}

pub static ORIGINS_LIST_2021: Lazy<HashSet<&str>> =
    Lazy::new(||{
        HashSet::from([
            "https://github.com/ActivityWatch/activitywatch",
            "https://github.com/AlessandroZ/LaZagne",
            "https://github.com/apenwarr/sshuttle",
            "https://github.com/brennerm/PyTricks",
            "https://github.com/crazyguitar/pysheeet",
            "https://github.com/Dman95/SASM",
            "https://github.com/easy-tensorflow/easy-tensorflow",
            "https://github.com/Eloston/ungoogled-chromium",
            "https://github.com/EpistasisLab/tpot",
            "https://github.com/formspree/formspree",
            "https://github.com/Guake/guake",
            "https://github.com/HIT-SCIR/ltp",
            "https://github.com/jaungiers/LSTM-Neural-Network-for-Time-Series-Prediction",
            "https://github.com/Jrohy/multi-v2ray",
            "https://github.com/lazyprogrammer/machine_learning_examples",
            "https://github.com/linkedin/qark",
            "https://github.com/metabrainz/picard",
            "https://github.com/miguelgrinberg/microblog",
            "https://github.com/mininet/mininet",
            "https://github.com/misterch0c/shadowbroker",
            "https://github.com/momosecurity/aswan",
            "https://github.com/MycroftAI/mycroft-core",
            "https://github.com/n1nj4sec/pupy",
            "https://github.com/PaddlePaddle/Paddle",
            "https://github.com/python/cpython",
            "https://github.com/scikit-learn-contrib/imbalanced-learn",
            "https://github.com/scrapinghub/portia",
            "https://github.com/SpiderLabs/Responder",
            "https://github.com/stanfordnlp/stanza",
            "https://github.com/StevenBlack/hosts",
            "https://github.com/Tribler/tribler",
            "https://github.com/Yorko/mlcourse.ai",
            "https://gitlab.com/EAVISE/brambox.git",
            "https://gitlab.com/elixire/elixire.git"
        ])
    });
