use regex::Regex;
use once_cell::sync::Lazy;

pub static b2: Lazy<Regex> = Lazy::new(|| Regex::new(r"^refs/heads/(main|master|dev|devel|develop|development)$").unwrap()); 

fn main(){ 
    let branch_name = "refs/heads/development";
    if !b2.is_match(&branch_name){
        println!("no match");
    }
    else{
        println!("match")
    }
}