use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering;
use swh_graph::graph::*;
use swh_graph::labels::EdgeLabel;
use swh_graph::NodeType;
use log::warn;
use crate::env;

#[derive(Debug, Clone)]
pub struct FileSystemTree {
    pub directories: HashMap<String, DirectoryInfo>,
    pub files: HashMap<String, FileInfo>,
    // Reverse lookups for performance
    pub node_to_dir_path: HashMap<usize, String>,
    pub node_to_file_path: HashMap<usize, String>,
}

#[derive(Debug, Clone)]
pub struct DirectoryInfo {
    pub path: String,
    pub node_id: usize,
    pub parent: Option<String>,
    pub children: Vec<String>, // paths of children
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub filename: String,
    pub node_id: usize,
    pub parent_dir: String,
    pub status: env::SubCateg,
}

impl FileSystemTree {
    pub fn new() -> Self {
        Self {
            directories: HashMap::new(),
            files: HashMap::new(),
            node_to_dir_path: HashMap::new(),
            node_to_file_path: HashMap::new(),
        }
    }
    
    pub fn add_directory(&mut self, dir_info: DirectoryInfo) {
        let path = dir_info.path.clone();
        let node_id = dir_info.node_id;
        self.node_to_dir_path.insert(node_id, path.clone());
        self.directories.insert(path, dir_info);
    }
    
    pub fn add_file(&mut self, file_info: FileInfo) {
        let path = file_info.path.clone();
        let node_id = file_info.node_id;
        self.node_to_file_path.insert(node_id, path.clone());
        self.files.insert(path, file_info);
    }
    
    // Check if a node_id with specific name exists
    pub fn has_node_with_name(&self, node_id: usize, name: &str) -> bool {
        // Check if it's a directory
        if let Some(dir_path) = self.node_to_dir_path.get(&node_id) {
            if let Some(dir) = self.directories.get(dir_path) {
                return dir.path.split('/').last().unwrap_or("") == name;
            }
        }
        
        // Check if it's a file
        if let Some(file_path) = self.node_to_file_path.get(&node_id) {
            if let Some(file) = self.files.get(file_path) {
                return file.filename == name;
            }
        }
        
        false
    }
    
    // Get node info by node_id
    pub fn get_node_info(&self, node_id: usize) -> Option<(&str, bool)> {
        // Returns (path, is_directory)
        if let Some(path) = self.node_to_dir_path.get(&node_id) {
            return Some((path, true));
        }
        if let Some(path) = self.node_to_file_path.get(&node_id) {
            return Some((path, false));
        }
        None
    }
    
    // Check if a file exists at a specific path
    pub fn has_file_at_path(&self, path: &str) -> Option<usize> {
        self.files.get(path).map(|file| file.node_id)
    }
    
    // Check if a directory exists at a specific path
    pub fn has_directory_at_path(&self, path: &str) -> Option<usize> {
        self.directories.get(path).map(|dir| dir.node_id)
    }

    pub fn get_all_file_paths_in_directory(&self, dir_path: &str) -> Vec<String> {
        let mut file_paths = Vec::new();
        let mut to_visit = VecDeque::new();
        to_visit.push_back(dir_path);
        
        while let Some(current_path) = to_visit.pop_front() {
            if let Some(dir_info) = self.directories.get(current_path) {
                for child_path in &dir_info.children {
                    if self.files.contains_key(child_path) {
                        file_paths.push(child_path.clone());
                    } else if self.directories.contains_key(child_path) {
                        to_visit.push_back(child_path);
                    }
                }
            }
        }
        file_paths
    }

    pub fn mark_directory_contents_as_found(&mut self, dir_path: &str) {
        let files = self.get_all_file_paths_in_directory(dir_path);
        for file in files {
            if let Some(file_info) = self.files.get_mut(&file){
                file_info.status = env::SubCateg::FileFound;
            }
        }
    }
}

pub fn get_list_of_changes(
    fs: &FileSystemTree,
) -> HashSet<env::SubCateg>
{
    let mut res = HashSet::new();
    for file in fs.files.values(){
        if file.status != env::SubCateg::FileFound {
            res.insert(file.status);
        }
        if res.iter().count() > 1{
            return res;
        }
    }
    res
}

pub fn get_list_of_content<G: SwhLabeledForwardGraph + SwhGraphWithProperties>(
    dir: usize,
    graph: &G,
) -> FileSystemTree
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut filesystem = FileSystemTree::new();
    
    // Add root directory
    let root_dir = DirectoryInfo {
        path: ".".to_string(),
        node_id: dir,
        parent: None,
        children: Vec::new(),
    };
    filesystem.add_directory(root_dir);
    
    // Track paths during traversal
    let mut path_node: HashMap<usize, String> = HashMap::new();
    path_node.insert(dir, ".".to_string());
    
    let mut to_visit = VecDeque::new();
    to_visit.push_back(dir);
    let mut visited = HashSet::new();
    
    while let Some(node) = to_visit.pop_front() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        
        let current_path = path_node.get(&node).expect("couldn't find path in path_node").clone();
        let mut children_paths = Vec::new();
        
        for (succ, labels) in graph.labeled_successors(node) {
            for label in labels {
                let name: String;
                if let EdgeLabel::DirEntry(dir_entry) = label {
                    name = String::from_utf8_lossy(
                        &graph.properties().label_name(dir_entry.filename_id())
                    ).to_string();
                } else {
                    continue;
                }
                
                let path = if current_path == "." {
                    name.clone()
                } else {
                    format!("{}/{}", current_path, name)
                };
                
                children_paths.push(path.clone());
                
                match graph.properties().node_type(succ) {
                    NodeType::Content => {
                        // Add file to filesystem
                        let file_info = FileInfo {
                            path: path.clone(),
                            filename: name,
                            node_id: succ,
                            parent_dir: current_path.clone(),
                            status: env::SubCateg::FileRemoved,
                        };
                        filesystem.add_file(file_info);
                    }
                    NodeType::Directory => {
                        // Add directory to filesystem
                        let dir_info = DirectoryInfo {
                            path: path.clone(),
                            node_id: succ,
                            parent: Some(current_path.clone()),
                            children: Vec::new(), // Will be filled as we traverse
                        };
                        filesystem.add_directory(dir_info);
                        
                        path_node.insert(succ, path);
                        to_visit.push_back(succ);
                    }
                    _ => continue,
                }
            }
        }
        
        // Update parent directory with children
        if let Some(dir_info) = filesystem.directories.get_mut(&current_path) {
            dir_info.children = children_paths;
        }
    }
    
    filesystem
}

pub fn dir_changes<G: SwhLabeledForwardGraph + SwhGraphWithProperties + SwhLabeledBackwardGraph>(
    fs: &mut FileSystemTree,
    rev_swhid: &str,
    snap_dst: &str,
    branch: &str,
    graph_t: &G,
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let props = graph_t.properties();

    if !crate::classes::check_branch(snap_dst, branch, graph_t){
        return;
    }

    let rev_mc: usize = graph_t
        .properties()
        .node_id(rev_swhid)
        .expect("Couldn't find node id");
    
    let mut succs = HashSet::new();
    // find all successors commits or None is it's the initial commit
    // successors should be in snap_dst since we focused on root cause commits
    // if there is no successor, then we compare it with the root commit of snap_dst
    match crate::classes::find_successors(rev_mc, graph_t) {
        Some(findings) => {
            succs.extend(findings);
        }
        None => {
            succs.insert(crate::classes::find_initial_rev(snap_dst, graph_t).expect("Can't find initial commit"));
        }
    }

    if let Some(rev_to_explore) = crate::classes::find_revs(rev_mc, succs, branch, snap_dst, graph_t){
        rev_to_explore.into_iter().for_each(|rev|{
            if let Some(dir_dst) = crate::classes::get_dir(props.swhid(rev).to_string().as_str(), graph_t) {
                // todo!("finish compare_dir and what to do with it in this function");
                compare_dir(fs, dir_dst, graph_t);
            } else {
                warn!("couldn't find dir for rev {}", props.swhid(rev).to_string());
                env::REV_WITHOUT_DIR.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
}

fn compare_dir<G: SwhLabeledForwardGraph + SwhGraphWithProperties + SwhLabeledBackwardGraph>(
    fs: &mut FileSystemTree,
    dir_to_compare: usize, 
    graph: &G,
)
where
    <G as SwhGraphWithProperties>::Maps: swh_graph::properties::Maps,
    <G as SwhGraphWithProperties>::LabelNames: swh_graph::properties::LabelNames,
    <G as SwhGraphWithProperties>::Strings: swh_graph::properties::Strings,
    <G as SwhGraphWithProperties>::Persons: swh_graph::properties::Persons,
    <G as SwhGraphWithProperties>::Timestamps: swh_graph::properties::Timestamps,
{
    let mut path_node = HashMap::new();
    path_node.insert(dir_to_compare, String::from("."));
    
    let mut to_visit = VecDeque::new();
    to_visit.push_back(dir_to_compare);
    let mut visited = HashSet::new();
    while let Some(node) = to_visit.pop_front(){
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);
        for (succ, labels) in graph.labeled_successors(node){
            for label in labels{
                let name: String;
                if let EdgeLabel::DirEntry(dir) = label{
                    name = String::from_utf8_lossy(&graph.properties().label_name(dir.filename_id())).to_string();

                }else{
                    continue;
                }
                let path = format!(
                    "{}/{}",
                    path_node
                        .get(&node)
                        .expect("couldn't find path in path_node"),
                    name
                );
                match graph.properties().node_type(succ){
                    NodeType::Content =>{
                        if let Some(file_info) = fs.files.get_mut(&path) {
                            if file_info.node_id == succ {
                                file_info.status = env::SubCateg::FileFound;
                            } else {
                                file_info.status = env::SubCateg::FileModified;
                            }
                        }
                    },
                    NodeType::Directory =>{
                        path_node.insert(succ, path.clone());
                        if let Some(dir) = fs.directories.get_mut(&path){
                            if dir.node_id == succ{
                                fs.mark_directory_contents_as_found(&path);
                                continue;
                            }
                        }
                        to_visit.push_back(succ);
                    }
                    _ => continue,
                }
            }
        }
    }
}