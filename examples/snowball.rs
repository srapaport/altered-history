use std::fs;
use altered_history::env;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use counter::Counter;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Serialize;

const RESULTS_PATH: &str = "/home/infres/rapaport/results/FULL_2024_08_v2"; // FULL
// const RESULTS_PATH: &str = "/home/infres/rapaport/results/TEASE_2024_08_23_new"; // TEASE

fn main(){
    snowball();
}

fn snowball(){
    let mut cpt = 0;
    let mut all_res: Counter<String, usize> = Counter::new();
    let bar = ProgressBar::new(12_542_848_352 - 1);
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("loading all_res");
    fs::read_dir(RESULTS_PATH)
        .expect("can't read dir")
        .into_iter()
        .for_each(|file|{
            let filename = &file
                .unwrap()
                .path()
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap();
            if env::RE_CSV.is_match(&filename){
                csv::ReaderBuilder::new()
                    .has_headers(true)
                    .delimiter(b';')
                    .from_path(format!("{RESULTS_PATH}/{filename}"))
                    .unwrap()
                    .deserialize()
                    .into_iter()
                    .for_each(|result: Result<env::AlteredCommit, csv::Error>| {
                        cpt += 1;
                        if let Ok(record) = result{
                            all_res[&record.origin] += 1;
                        }
                        else {
                            info!("not the right amount of column: {cpt}");
                        }
                        bar.inc(1);
                    });
            }
        });
    bar.finish();

    let bar = ProgressBar::new(10_264_390 - 1);
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("loading focus_res");
    let mut focus_res: Counter<String, usize> = Counter::new();
    fs::read_dir(format!("{RESULTS_PATH}/focus"))
        .expect("can't read dir")
        .into_iter()
        .for_each(|file|{
            let filename = &file
                .unwrap()
                .path()
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap();
            if env::RE_CSV.is_match(&filename){
                csv::ReaderBuilder::new()
                    .has_headers(true)
                    .delimiter(b';')
                    .from_path(format!("{RESULTS_PATH}/focus/{filename}"))
                    .unwrap()
                    .deserialize()
                    .into_iter()
                    .for_each(|result: Result<env::AlteredCommit, csv::Error>| {
                        cpt += 1;
                        if let Ok(record) = result{
                            focus_res[&record.origin] += 1;
                        }
                        else {
                            info!("not the right amount of column: {cpt}");
                        }
                        bar.inc(1);
                    });
            }
        });
    bar.finish();
    
    #[derive(Serialize, Debug, Clone)]
    struct Package{
        url: String,
        mean: f64,
    }

    let mut csv_wrt = csv::WriterBuilder::new()
        .from_path(format!("{RESULTS_PATH}/logs/snowball.csv"))
        .unwrap();

    let packages: Vec<Package> = focus_res.into_iter().collect::<Vec<_>>().into_par_iter()
    .filter_map(|elem| {
        all_res.get(&elem.0).map(|total| Package {
            url: elem.0,
            mean: (*total as f64) / (elem.1 as f64),
        })
    })
    .collect();

    let bar = ProgressBar::new(10_264_390 - 1);
    bar.set_style(
        ProgressStyle::with_template(
            "{msg} {wide_bar} {pos} {percent_precise}% {elapsed_precise} {eta}",
        )
        .unwrap(),
    );
    bar.set_message("writing res");
    for package in packages {
        csv_wrt.serialize(package.clone()).expect(&format!("failed serializing {}", package.url));
        bar.inc(1);
    }
    bar.finish();

}