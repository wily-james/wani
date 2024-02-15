mod wanidata;

use crate::wanidata::{
    WaniData,
    WaniResp
};
use std::{fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use clap::{Parser, Subcommand};
use chrono::Utc;
use reqwest::{
    blocking::Client, StatusCode
    //Error,
};
use rusqlite::Connection;

#[derive(Parser)]
struct Args {
    /// Subcommand to run. Default is summary
    #[command(subcommand)]
    command: Option<Command>,

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

#[derive(Subcommand)]
enum Command {
    /// Lists a summary of the Lessons and Reviews that are currently available. This is the
    /// default command
    Summary,
    /// a shorthand for the 'summary' command
    S,
    /// Does first-time initialization
    Init,
    TestSubject,
}

struct ProgramConfig {
    auth: Option<String>,
}

#[derive(Debug)]
struct Error {
    msg: String
}

fn main() {
    let args = Args::parse();

    match &args.command {
        Some(c) => {
            match c {
                Command::Summary => wani_summary(&args),
                Command::S => wani_summary(&args),
                Command::Init => wani_init(&args),
                Command::TestSubject => wani_test_subject(&args)
            }
        },
        None => wani_summary(&args),
    }

}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn setup_connection(args: &Args) -> Result<Connection, Error> {
    let mut datapath = PathBuf::new();
    if let Some(dpath) = &args.datapath {
        datapath.push(dpath);
    }
    else {
        match home::home_dir() {
            Some(h) => {
                datapath.push(h);
                datapath.push(".wani");
            },
            None => {
                return Err(Error { msg: "Could not find home directory. Please manually specify datapath arg. Use \"wani -help\" for more details.".into() });
            }
        }
    }
    
    if !Path::exists(&datapath)
    {
        if let Err(s) = fs::create_dir(&datapath) {
            return Err(Error { msg: format!("Could not create datapath at {}\nError: {}", datapath.display(), s) });
        }
    }

    let mut db_path = PathBuf::from(&datapath);
    db_path.push("wani_cache.db");

    match Connection::open(&db_path) {
        Err(e) => Err(Error { msg: format!("{}", e) }),
        Ok(c) => Ok(c),
    }
}

fn wani_init(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e.msg),
        Ok(c) => {

        },
    };
}

fn wani_test_subject(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e.msg);
        return;
    }

    let web_config = web_config.unwrap();
    let client = Client::new();
    let response = client
        .get("https://api.wanikani.com/v2/subjects")
        .header("Wanikani-Revision", "20170710")
        .query(&[("levels", "5")])
        .bearer_auth(web_config.auth)
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
                    println!("HTTP 401: Unauthorized. Make sure your wanikani auth token is correct, and hasn't been expired.");
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    println!("Wanikani API rate limit exceeded.");
                },
                _ => { println!("HTTP status code {}", r.status()) },
            };
        },
    }
}

struct WaniWebConfig {
    auth: String
}

fn get_web_config(args: &Args) -> Result<WaniWebConfig, Error> {
    let mut configpath = PathBuf::new();
    if let Some(path) = &args.configfile {
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
                        return Err(Error { msg: format!("Could not create wani config folder at {}\nError: {}", configpath.display(), s) });
                    }
                }
                configpath.push(".wani.conf");
            },
            None => {
                return Err(Error { msg: format!("Could not find home directory. Please manually specify configpath arg. Use \"wani -help\" for more details.") });
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
        return Err(Error { msg: format!("Need to specify a wanikani access token") });
    }

    return Ok(WaniWebConfig { auth });
}

fn wani_summary(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e.msg);
        return;
    }
    let web_config = web_config.unwrap();
    let client = Client::new();
    let response = client
        .get("https://api.wanikani.com/v2/summary")
        .header("Wanikani-Revision", "20170710")
        .bearer_auth(web_config.auth)
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
                    println!("HTTP 401: Unauthorized. Make sure your wanikani auth token is correct, and hasn't been expired.");
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

        WaniData::Collection(collection) => {
            println!("Collection: ");
            for data in collection.data {
                println!("{:?}", data);
            }
        },

        _ => {
            println!("Unexpected response type");
        }
    }
}
