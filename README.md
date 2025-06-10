# Altered History

**Altered History** is a Rust-based tool for analyzing version control history alterations in software repositories archived by [Software Heritage](https://www.softwareheritage.org/). This project detects, extracts, and categorize commits that have been modified or "altered" from their original history within Software Heritage's compressed graph datasets.

## What Does This Project Do?

This tool performs a comprehensive analysis of Git history alterations by:

1. **Detecting Altered Commits**: Identifies commits that appear to have been modified, removed, or restructured in the version control history
2. **Root Cause Commits Identification**: Pinpoints the original commits that initiated the "snowball effect" of history alterations
3. **Categorization**: Categorizes alterations by type (metadata changes, directory changes, loading issues, etc.) and specific subcategories

### Key Concepts

- **Altered Commits**: Commits that have been modified from their original form, either through Git operations like rebase, squash, or manual history editing
- **Root Cause Commits**: The original commits that initiated a chain of history alterations
- **Software Heritage Graph**: A compressed representation of software repositories that preserves the complete development history

## How It Works

The tool analyzes Software Heritage's compressed graph datasets to:

1. **Compare Snapshots**: Examines different snapshots of the same repository over time
2. **Identify Missing Commits**: Detects commits that appear in some snapshots but not others
3. **Trace History**: Follows the Git history to understand how alterations propagated
4. **Categorize Changes**: Determines the nature and cause of each alteration

### Three-Step Analysis Process

The analysis is performed in three distinct steps:

#### Step 1: Initial Detection
Scans through all origins in the Software Heritage graph to identify repositories with altered histories. This step processes each repository's snapshots chronologically to detect missing or modified commits.

#### Step 2: Root Cause Commits Identification
Focuses on the detected alterations to identify the root cause commits - those that initiated the chain of changes. This eliminates noise and focuses on the commits that were actually manually altered.

#### Step 3: Categorization
Categorizes each root cause alteration by analyzing the specific changes made, providing insights into why and how the history was altered.

## Classification System

### Main Categories

- **META**: Changes to commit metadata (author, message, dates, etc.)
- **DIR**: Changes to directory structure or file content
- **LoadingIssue**: Technical issues preventing proper analysis

### Sub-Categories

#### Metadata Changes (META)
- `Message`: Commit message was modified
- `Author`: Author information was changed
- `Date`: Commit date was altered
- `Committer`: Committer information was changed
- `CommitterDate`: Committer date was modified

#### Directory/Content Changes (DIR)
- `FileModified`: File contents were changed
- `FileRemoved`: Files were deleted from the commit
- `ContentSplit`: Content was split across multiple commits
- `DifferentBranchName`: Branch names were changed

## Getting Started

### Prerequisites

- Rust toolchain (latest stable version)
- Software Heritage compressed graph dataset
- For datasets created before 2024-05: ORC file `origin_visit_status`

### Build

```sh
cargo build --release
```

### Datasets

Download compressed graph datasets from [Software Heritage's website](https://docs.softwareheritage.org/devel/swh-dataset/graph/dataset.html#popular-3k-python).

## Usage

### Configuration

The tool uses a configuration structure:

```rust
pub struct Options {
    /// Directory containing the ORC files (for old datasets)
    pub orc_dir: String,
    /// Directory for temporary database files (no trailing slash)
    pub output_dir: String,
    /// Path to the compressed graph (e.g., "./datasets/2021-03-23-popular-3k-python-graph/graph")
    pub graph: String,
    /// Path where results are stored (no trailing slash, must exist)
    pub results: String,
    /// Number of origins in the graph
    pub expected_origins: usize,
    /// Number of origins per result file (for chunking output)
    pub chunk: usize,
}
```

### Step-by-Step Execution

#### Step 1: Detect Altered Commits

**For datasets created before 2024-05:**

```rust
// Process visit timestamps from ORC files
altered_history::visit_timestamps::retrieve_visit_timestamps(&opts);

// Run main analysis with database support
if let Some(cp) = altered_history::load_checkpoint(&opts) {
    altered_history::main_all_database_mpsc_with_cp(&opts, cp);
} else {
    altered_history::main_all_database_mpsc(&opts);
}
```

**For recent datasets (2024-05 and later):**

```rust
// Run main analysis without database dependency
if let Some(cp) = altered_history::load_checkpoint(&opts) {
    altered_history::main_all_mpsc_with_cp(&opts, cp);
} else {
    altered_history::main_all_mpsc(&opts);
}
```

Results are stored in CSV files at `{results_path}/`.

#### Step 2: Root Cause Analysis

```rust
altered_history::analysis::focus_missing_commits_all_files_with_save(&opts);
```

This step processes the results from Step 1 and identifies the root cause commits. Results are stored at `{results_path}/focus/`.

#### Step 3: Categorization

```rust
altered_history::classes::categorization_all(&opts);
```

Classifies each root cause alteration by type and specific changes. Results are stored at `{results_path}/focus/classes/`.

### Directory Setup

Create the required directory structure before running:

```sh
mkdir -p results/focus/classes
```

### Command Line Execution

If you choose to use `clap::parse`

```sh
./target/release/altered_history \
    <orc_file_path> \
    <database_output_path> \
    <graph_path> \
    <results_path> \
    <number_of_origins> \
    <chunk_size>
``` 

Otherwise, just fill the `src/main.rs` file.

### Example

```sh
./target/release/altered_history \
    ./orc_files \
    /tmp/altered_history_db \
    ./datasets/2021-03-23-popular-3k-python/graph \
    ./results/2021_analysis \
    3000 \
    100
```

## Output Format

### Step 1 Output
CSV files containing detected alterations:
```
origin,snapshot_src,branch_name,missing_commit,snapshot_dst
```

### Step 2 Output (Root Cause Commits)
CSV files containing root cause commits:
```
origin,snapshot_src,branch_name,missing_commit,snapshot_dst
```

### Step 3 Output (Categorization)
CSV files with detailed classification:
```
origin,snapshot_src,branch_name,missing_commit,snapshot_dst,first_difference,main_category,sub_categories
```

## Performance Considerations

- **Multithreading**: The tool uses parallel processing to handle large datasets efficiently
- **Checkpointing**: Supports resuming interrupted analysis from checkpoints
- **Chunking**: Results are written in chunks to manage memory usage
- **Progress Tracking**: Provides real-time progress bars for long-running operations
