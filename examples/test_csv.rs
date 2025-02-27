use csv;
use serde::{Deserialize, Serialize};
fn main(){ 
    altered_history::orc_gen::csv_to_orc("./test.csv", "./test.orc");    
}

fn _test(){
    #[derive(Serialize, Deserialize, Debug)]
    struct ReadTest{
        origin: String,
        amount_of_snapshots: usize,
    }

    let mut csv_wrt = csv::Writer::from_path("./examples/test.csv").unwrap();
    //csv_wrt.write_record(&["origin", "amount of snapshots"]).unwrap();
    csv_wrt.serialize(ReadTest{origin: "test1".to_string(), amount_of_snapshots: 31}).unwrap();
    csv_wrt.serialize(ReadTest{origin: "test12".to_string(), amount_of_snapshots: 42}).unwrap();
    csv_wrt.flush().unwrap();

    let mut csv_rdr = csv::ReaderBuilder::new().has_headers(true).double_quote(false).from_path("./examples/test.csv").unwrap();
    for result in csv_rdr.deserialize(){
        let record: ReadTest = result.unwrap();
        println!("result: {:#?}", record);
    }
}