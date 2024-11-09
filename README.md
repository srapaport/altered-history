# Altered History

This project aims to retrieve all altered commits from a given compressed graph that one can download on [Software Heritage's website](https://docs.softwareheritage.org/devel/swh-dataset/graph/dataset.html#popular-3k-python).

## Getting started
For old datasets (created before 2024-05) the extraction requires also the ORC file `origin_visit_status`.

### Build
In the clone repository:
```sh
cargo build --release
```
### How to use
There are 3 steps to retrieve the dataset of altered commits.

#### Step 1
**If you analyze an old dataset (<2024-05):**

You then need to put the path of the orc file `origin_visit_status` as the first argument and the path of where a temporary database (build from the orc file to retrieve the data, basically a `/tmp` should do it) should be built.

In the `src/main.rs` use the following functions:

```rs
let opts = altered_history::env::Options::parse();

altered_history::visit_timestamps::retrieve_visit_timestamps(&opts);

if let Some(cp) = altered_history::load_checkpoint(&opts){
    altered_history::main_all_database_mpsc_with_cp(&opts, cp);
}
else{
    altered_history::main_all_database_mpsc(&opts);
} 
```

**If you use a recent dataset:**

In the `src/main.rs` use the following functions:

```rs
if let Some(cp) = altered_history::load_checkpoint(&opts){
    altered_history::main_all_mpsc_with_cp(&opts, cp);
}
else{
    altered_history::main_all_mpsc(&opts);
} 
```

Results will be stored at the path you give as the 4th parameter `/results`.

#### Step 2

To retrieve all the root causes of altered commits (commits that initated the snowball effect) use the following function:

```rs
altered_history::analysis::focus_missing_commits_all_files_with_save(&opts);
```

Results will be stored in a sub-directory of the path you give as the 4th parameter `/results/focus`.

#### Step 3

To retrieve all the root causes of altered commits classified use the following function:

```rs
classes::classification_all(&opts);
```

Results will be stored in a sub-directory of the path you give as the 4th parameter `/results/focus/classes`.

#### Execute

Always build the `--release` version of this project.

The results directory must be already created with two sub-directory inside:

```sh
mkdir results
mkdir results/focus
mkdir results/focus/classes
```

Execute the program with all the right inputs:

```rs
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
```

```sh
./target/release/altered_history <orc file path> <orc file database output> <graph path> <results path> <amount of origins within the graph> <amount of results per file>
```