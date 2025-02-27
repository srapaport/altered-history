use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::LevelFilter;
use std::{collections::{HashMap, HashSet}, path::PathBuf};
use swh_graph::{
    graph::{SwhBackwardGraph, SwhBidirectionalGraph, SwhGraphWithProperties}, mph::DynMphf, NodeType, SwhGraphProperties
};
use log::debug;
pub static ERRORS: [&str; 37] = [
    "bd431c96e00814cf274b5f1f6c46c64f583e225e",
"9f40f9d8e90cb222262ef477678f5012f86ae831",
"63dc717298c8d42ef3e2dcf144b17c4bf06512d8",
"24df863ec0f22cfa03dfe44b542041555fb60902",
"c00b086a832ed881f51d24ffa0a0349c8b778120",
"5ac093675beecee0e2285dfe51c6c19c038d76a8",
"aa538f88b6444d3c4594608d24b56a2f93fc852d",
"232e384606abe235abda5995dc086cf450312700",
"8f3bd508e90d05d5a732c004254dc8cf9f97de40",
"ecd64424f0a5d515830135528355c686155388f7",
"bca932e110901fbaa9f29dcf981d05cd97b474b0",
"f96a0936771b773b09e16ae0b2d92499b77e39f0",
"1ebde4c2ca1bbc113776c285f15a57a2d618858c",
"fb21c344c6d2b875a50576dcf1991df0b05f62cd",
"ba3343bc4fa403a8dfbfcab7fc1a8c29ee34bd69",
"d01a98b5afbc939f860c433a90a03ed17dfe74de",
"66764b585adf075ab20b7494c04283707f6143ca",
"27f27e92bdf2b140e8bb9b8a1c6e642c5d4a20f2",
"422d1e47ccca37b0ccce61d3bbb957f24a73f5c5",
"0f3d534237885f58cb336b112ea5af5aaa98f50f",
"dfd645e78e158a80e5b363d1f324303609be48c7",
"e892f06fb280012cf6ece80e993deb4c482c0a9c",
"e403e55a6810ee67a31d8b066f16416ce365d28d",
"b36a8bbcc8816993d55b69db4e0f74bcf1086243",
"796ecc0c56941a87963f4d78e11975636a9dbfed",
"f4c832f2b4fad84687c3506b232ab8d3b2dea081",
"06f5cd728667bd7001f164da36f260a43f3b384c",
"7a78b73c2133a8d8b851bc9542a5d3ba1b2b690f",
"03e039cc75bdf4fe0ba50330ccb2fbad27135d5e",
"114514a0c8259cd395b4e16acd78a89c2e317f5f",
"4cfe4cb4e202f64de8745f9f5ea64723939fb7cd",
"16319b9b5c3fecd40496d0d1728a5b7ffa53c3db",
"faa32850f596d8db51028b48bdbc5463749a453f",
"db4ba881c4e239b6717606272ae9131738111980",
"ab3e4391b5f687d1a787f24263dc71e63e12203c",
"4ff9acca1bde0d232b3401ea79901f8f4729a3e7",
"e41517fb72f23d3c817028f0b47e2ef7de58f09c"
];

fn main(){
    Logger::with(LevelFilter::Debug)
    .log_to_file(
        flexi_logger::FileSpec::default()
            // .directory("/infres/ir800/rapaport/results/FULL_new/logs")
            // .basename("FULL-all")
            .directory("./test_logs")
            .basename("timestamp_error")
            .suffix("log"),
    )
    .rotate(
        Criterion::Size(10_000_000), // Rotate when the file exceeds 10 MB
        Naming::Numbers,             // Use numbers for rotated files
        Cleanup::KeepLogFiles(5),    // Keep at most 5 log files
    )
    .duplicate_to_stderr(Duplicate::Warn) // Duplicate logs to stderr
    .start()
    .unwrap();

    let graph = SwhBidirectionalGraph::new(PathBuf::from(
        "/poolswh/softwareheritage/graph/2024-08-23/compressed/graph",
    ))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels")
        .load_properties(SwhGraphProperties::load_strings)
        .expect("Could not load strings");


    let mut urls: HashMap<String, Vec<String>> = HashMap::new();
    ERRORS.iter().for_each(|commit|{
        debug!("Checking {}", *commit);
        let node_id = graph.properties().node_id(format!("swh:1:rev:{}", *commit).as_str()).unwrap();
        let mut visited = HashSet::new();
        let mut to_visit = Vec::new();
        to_visit.push(node_id);
        while let Some(node) = to_visit.pop(){
            if visited.contains(&node){
                continue;
            }
            visited.insert(node);
            for pred in graph.predecessors(node){
                debug!("pred: {}", graph.properties().swhid(pred).to_string());
                if graph.properties().node_type(pred) == NodeType::Origin{
                    let url = graph.properties().message(pred).unwrap();
                    urls.entry(String::from(*commit))
                        .or_insert(Vec::new())
                        .push(String::from_utf8(url).unwrap());
                }
                else{
                    to_visit.push(pred);
                }
            }
        }

    });
    println!("url;commit");
    urls.into_iter().for_each(|(commit, urls)|{
        urls.into_iter().for_each(|url|{
            println!("{};{}", url, commit);
        });
    });
}