use altered_history::db;
use counter::Counter;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Serialize;

const GRAPH_PATH: &str = "/poolswh/softwareheritage/graph/2025-05-18/compressed/graph";
const OUTPUT_PATH: &str = "/home/infres/rapaport/results/FULL_2025_05_18/logs/snowball.csv";

fn main() {
    let tables = db::TableNames::from_graph_path(GRAPH_PATH);
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let pool = rt
        .block_on(db::init_pool())
        .expect("Failed to initialize DB pool");

    println!("Using table: {}", tables.altered_histories);
    snowball(&pool, &tables, &rt);
}

fn snowball(pool: &sqlx::PgPool, tables: &db::TableNames, rt: &tokio::runtime::Runtime) {
    // Count all detected+root_cause+classified commits per origin (the full set before focus)
    println!("Counting all altered commits per origin...");
    let all_rows: Vec<(String, i64)> = rt
        .block_on(async {
            let q = format!(
                "SELECT origin, COUNT(*) as cnt FROM {} GROUP BY origin",
                tables.altered_histories
            );
            sqlx::query_as::<_, (String, i64)>(&q)
                .fetch_all(pool)
                .await
        })
        .expect("Failed to query all commits");

    let mut all_res: Counter<String, usize> = Counter::new();
    for (origin, cnt) in &all_rows {
        all_res[origin] += *cnt as usize;
    }
    println!("Loaded {} origins from all commits", all_res.len());

    // Count root_cause + classified commits per origin (the focused set)
    println!("Counting focused commits per origin...");
    let focus_rows: Vec<(String, i64)> = rt
        .block_on(async {
            let q = format!(
                "SELECT origin, COUNT(*) as cnt FROM {} WHERE status IN ('root_cause', 'classified') GROUP BY origin",
                tables.altered_histories
            );
            sqlx::query_as::<_, (String, i64)>(&q)
                .fetch_all(pool)
                .await
        })
        .expect("Failed to query focus commits");

    let mut focus_res: Counter<String, usize> = Counter::new();
    for (origin, cnt) in &focus_rows {
        focus_res[origin] += *cnt as usize;
    }
    println!("Loaded {} origins from focused commits", focus_res.len());

    #[derive(Serialize, Debug, Clone)]
    struct Package {
        url: String,
        mean: f64,
    }

    let mut csv_wrt = csv::WriterBuilder::new()
        .from_path(OUTPUT_PATH)
        .unwrap();

    let packages: Vec<Package> = focus_res
        .into_iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|elem| {
            all_res.get(&elem.0).map(|total| Package {
                url: elem.0,
                mean: (*total as f64) / (elem.1 as f64),
            })
        })
        .collect();

    let bar = ProgressBar::new(packages.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("writing res");
    for package in packages {
        csv_wrt
            .serialize(package.clone())
            .expect(&format!("failed serializing {}", package.url));
        bar.inc(1);
    }
    bar.finish();
}
