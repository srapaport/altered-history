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
If you need