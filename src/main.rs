use std::{fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use clap::Parser;
use serde::Deserialize;
use chrono::{
    DateTime,
    Utc,
};
use reqwest::{
    blocking::Client, StatusCode
    //Error,
};
use rusqlite::Connection;

#[derive(Deserialize, Debug)]
#[serde(tag="object")]
enum WaniData
{
    Collection,
    #[serde(rename="report")]
    Report(Summary),
}

#[derive(Debug, Deserialize)]
struct WaniResp {
    //url: String,
    //data_updated_at: Option<String>, // TODO - optional for collections if no elements, mandatory
    #[serde(flatten)]
    data: WaniData
}

#[derive(Deserialize, Debug)]
struct Summary {
    data: SummaryData
}

#[derive(Deserialize, Debug)]
struct SummaryData {
    lessons: Vec<Lesson>,
    //next_reviews_at: Option<String>,
    reviews: Vec<SummaryReview>
}

#[derive(Deserialize, Debug)]
struct SummaryReview {
    available_at: DateTime<Utc>,
    subject_ids: Vec<i32>,
}

#[derive(Deserialize, Debug)]
struct Lesson {
    available_at: DateTime<Utc>,
    subject_ids: Vec<i32>
}

#[derive(Parser)]
struct Args {
    /// Specifies the Wanikani API personal access token to use.
    /// See: https://www.wanikani.com/settings/personal_access_tokens
    #[arg(short, long, value_name = "TOKEN")]
    auth: Option<String>,

    /// Specifies the directory in which to locate/create a cache of Wanikani data. Default is ~/.wani
    #[arg(short, long, value_name = "PATH")]
    datapath: Option<PathBuf>,

    /// Specifies the file path for wani configuration. Default is ~/.config/wani/.wani.conf
    #[arg(short, long, value_name = "FILE")]
    configfile: Option<PathBuf>,
}

struct ProgramConfig {
    auth: Option<String>,
}

fn main() {
    wani_summary();
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn wani_summary() {
    let args = Args::parse();

    let mut configpath = PathBuf::new();
    if let Some(path) = args.configfile {
        configpath.push(path);
    }
    else {
        match home::home_dir() {
            Some(h) => {
                configpath.push(h);
                configpath.push(".config");
                configpath.push("wani");

                if !Path::exists(&configpath)
                {
                    if let Err(s) = fs::create_dir(&configpath) {
                        println!("Could not create wani config folder at {}\nError: {}", configpath.display(), s);
                        return;
                    }
                }
                configpath.push(".wani.conf");
            },
            None => {
                println!("Could not find home directory. Please manually specify configpath arg. Use \"wani -help\" for more details.");
                return;
            }
        };
    }

    let mut config = ProgramConfig { auth: None };
    if let Ok(lines) = read_lines(&configpath) {
        for line in lines {
            if let Ok(s) = line {
                let words = s.split(" ").collect::<Vec<&str>>();
                if words.len() < 2 {
                    continue;
                }

                match words[0] {
                    "auth:" => {
                        config.auth = Some(String::from(words[1]));
                    },
                    _ => {},
                }
            }
        }
    }
    else {
        println!("Error reading config at: {}", configpath.display());
    }

    let auth: String;
    if let Some(a) = &args.auth {
        auth = String::from(a);
    }
    else if let Some(a) = config.auth {
        auth = String::from(a);
    }
    else {
        println!("Need to specify a wanikani access token");
        return;
    }

    let mut datapath = PathBuf::new();
    if let Some(dpath) = args.datapath {
        datapath.push(dpath);
    }
    else {
        match home::home_dir() {
            Some(h) => {
                datapath.push(h);
                datapath.push(".wani");
            },
            None => {
                println!("Could not find home directory. Please manually specify datapath arg. Use \"wani -help\" for more details.");
                return;
            }
        }
    }
    
    if !Path::exists(&datapath)
    {
        if let Err(s) = fs::create_dir(&datapath) {
            println!("Could not create datapath at {}\nError: {}", datapath.display(), s);
            return;
        }
    }

    let mut db_path = PathBuf::from(&datapath);
    db_path.push("wani_cache.db");
    let conn = Connection::open(&db_path);
    if let Err(e) = conn {
        println!("{}", e);
        return;
    }
    let conn = conn.unwrap();

    let client = Client::new();
    let response = client
        .get("https://api.wanikani.com/v2/summary")
        .header("Wanikani-Revision", "20170710")
        .bearer_auth(auth)
        .send();

    match response {
        Err(s) => {
            println!("Error with request: {}", s)
        },

        Ok(r) => {
            match r.status() {
                StatusCode::OK => {
                    let wani = r.json::<WaniResp>();
                    match wani {
                        Err(s) => println!("Error parsing HTTP 200 response: {}", s),
                        Ok(w) => {
                            handle_wani_resp(w);
                        },
                    }
                },
                StatusCode::UNAUTHORIZED => {
                    println!("HTTP 401: Unauthorized");
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    println!("Wanikani API rate limit exceeded.");
                },
                _ => { println!("HTTP status code {}", r.status()) },
            };
        },
    }
}

fn handle_wani_resp(w: WaniResp) -> () {
    let now = Utc::now();
    match w.data {
        WaniData::Report(s) => {
            let mut count = 0;
            for lesson in s.data.lessons {
                if lesson.available_at < now {
                    count += lesson.subject_ids.len();
                }
            }

            println!("Lessons: {:?}", count);

            let mut count = 0;
            for review in s.data.reviews {
                if review.available_at < now {
                    count += review.subject_ids.len();
                }
            }

            println!("Reviews: {:?}", count);
        },

        _ => {
            println!("Unexpected response type");
        }
    }
}
