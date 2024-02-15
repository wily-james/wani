mod wanidata;
mod wanisql;

use crate::wanidata::WaniData;
use crate::wanidata::WaniResp;
use std::{fmt::Display, fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use clap::{Parser, Subcommand};
use chrono::Utc;
use reqwest::{
    blocking::Client, StatusCode
};
use rusqlite::params;
use rusqlite::{
    Connection, Error as SqlError
};
use thiserror::Error;

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
    /// Syncs local data with WaniKani servers
    Sync,
    /// Forces update of local data instead of only fetching new data
    ForceSync,

    // Debug/Testing commands:
    /// Check the cache info in db
    CacheInfo,
    QueryRadicals,
    QueryKanji,
    QueryVocab,
    TestSubject,
}

/// Info saved to program config file
struct ProgramConfig {
    auth: Option<String>,
}

/// Info needed to make WaniKani web requests
struct WaniWebConfig {
    auth: String
}


#[derive(Error, Debug)]
enum WaniError {
    Generic(String),
    Parse(#[from] serde_json::Error),
    Sql(#[from] SqlError),
    Chrono(#[from] chrono::ParseError),
}

impl Display for WaniError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaniError::Generic(g) => f.write_str(g),
            WaniError::Parse(e) => e.fmt(f),
            WaniError::Sql(e) => e.fmt(f),
            WaniError::Chrono(e) => e.fmt(f),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), WaniError> {
    let args = Args::parse();

    match &args.command {
        Some(c) => {
            match c {
                Command::Summary => command_summary(&args),
                Command::S => command_summary(&args),
                Command::Init => command_init(&args),
                Command::Sync => command_sync(&args, false),
                Command::ForceSync => command_sync(&args, true),

                // Testing
                Command::CacheInfo => command_cache_info(&args),
                Command::QueryRadicals => command_query_radicals(&args),
                Command::QueryKanji => command_query_kanji(&args),
                Command::QueryVocab => command_query_vocab(&args),
                Command::TestSubject => command_test_subject(&args),
            };
        },
        None => command_summary(&args),
    };

    Ok(())
}

fn command_query_vocab(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_ALL_VOCAB).unwrap();
            match stmt.query_map([], |v| wanisql::parse_vocab(v)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(vocab) => {
                    for v in vocab {
                        println!("{:?}", v)
                    }
                },
                Err(_) => {},
            };
        },
    };

}

fn command_query_kanji(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_ALL_KANJI).unwrap();
            match stmt.query_map([], |k| wanisql::parse_kanji(k)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(kanji) => {
                    for k in kanji {
                        println!("{:?}", k)
                    }
                },
                Err(_) => {},
            };
        },
    };

}

fn command_query_radicals(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_ALL_RADICALS).unwrap();
            match stmt.query_map([], |r| wanisql::parse_radical(r)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(radicals) => {
                    for r in radicals {
                        println!("{:?}", r)
                    }
                },
                Err(_) => {},
            };
        },
    };
}

fn command_cache_info(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            match c.query_row("select * from cache_info where id = 0", [], 
                        |r| Ok((r.get::<usize, i32>(0)?, r.get::<usize, Option<String>>(1)?, r.get::<usize, Option<String>>(2)?, r.get::<usize, Option<String>>(3)?))) {
                Ok(t) => {
                    println!("CacheInfo: Type {}, ETag {}, LastMod {}, UpdatedAfter {}", 
                             t.0, 
                             t.1.unwrap_or("None".into()), 
                             t.2.unwrap_or("None".into()), 
                             t.3.unwrap_or("None".into()));
                },
                Err(e) => {
                    println!("Error checking cache_info: {}", e);
                }
            }
            c.close().unwrap();
        },
    };
}

fn command_sync(args: &Args, ignore_cache: bool) {
    fn sync(args: &Args, conn: &Connection, ignore_cache: bool) {
        let web_config = get_web_config(&args);
        if let Err(e) = web_config {
            println!("{}", e);
            return;
        }

        let mut last_modified: Option<String> = None;
        let mut updated_after: Option<String> = None;
        if !ignore_cache {
            let res = conn.query_row("select i.last_modified, i.updated_after from cache_info i where id = 0;",
                                     [],
                                     |r| Ok((r.get::<usize, Option<String>>(0), r.get::<usize, Option<String>>(1))));
            match res {
                Ok(t) => {
                    if let Ok(tag) = t.0 {
                        last_modified = tag;
                    }
                    if let Ok(after) = t.1 {
                        updated_after = after;
                    }
                },
                Err(e) => {
                    println!("Error fetching cache_info. Error: {}", e);
                    return;
                }
            }
        }

        let web_config = web_config.unwrap();
        let client = Client::new();
        let mut request = client
            .get("https://api.wanikani.com/v2/subjects")
            .header("Wanikani-Revision", "20170710")
            .bearer_auth(web_config.auth);

        if let Some(tag) = last_modified {
            request = request.header(reqwest::header::LAST_MODIFIED, &tag);
        }

        if let Some(after) = updated_after {
            request = request.query(&[("updated_after", &after)]);
        }

        let request_time = Utc::now();
        match parse_response(request.send()) {
            Ok(t) => {
                let wr = t.0;
                let headers = t.1;

                match wr.data {
                    WaniData::Collection(c) => {
                        let mut parse_fails = 0;
                        let mut radicals: Vec<wanidata::Radical> = vec![];
                        let mut kanji: Vec<wanidata::Kanji> = vec![];
                        let mut vocab: Vec<wanidata::Vocab> = vec![];
                        let mut kana_vocab: Vec<wanidata::KanaVocab> = vec![];
                        for wd in c.data {
                            match wd {
                                WaniData::Radical(r) => {
                                    radicals.push(r);
                                }, 
                                WaniData::Kanji(k) => {
                                    kanji.push(k);
                                },
                                WaniData::Vocabulary(v) => {
                                    vocab.push(v);
                                },
                                WaniData::KanaVocabulary(kv) => {
                                    kana_vocab.push(kv);
                                },
                                _ => {},
                            }
                        }

                        let stmt_res = conn.prepare(wanisql::INSERT_RADICALS);
                        let rad_len = radicals.len();
                        match stmt_res {
                            Ok(mut stmt) => {
                                for r in radicals {
                                    match wanisql::store_radical(r, &mut stmt) {
                                        Err(e) => {
                                            println!("Error inserting into radicals.\n{}", e);
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error preparing insert into radicals statement. Error: {}", e);
                                parse_fails += rad_len;
                            }
                        }

                        let stmt_res = conn.prepare(wanisql::INSERT_KANJI);
                        let kanji_len = kanji.len();
                        match stmt_res {
                            Ok(mut stmt) => {
                                for k in kanji {
                                    match wanisql::store_kanji(k, &mut stmt) {
                                        Err(e) => {
                                            println!("Error inserting into kanji.\n{}", e);
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error preparing insert into kanji statement. Error: {}", e);
                                parse_fails += kanji_len;
                            }
                        }

                        let stmt_res = conn.prepare(wanisql::INSERT_VOCAB);
                        let vocab_len = vocab.len();
                        match stmt_res {
                            Ok(mut stmt) => {
                                for v in vocab {
                                    match wanisql::store_vocab(v, &mut stmt) {
                                        Err(e) => {
                                            println!("Error inserting into vocab.\n{}", e);
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error preparing insert into vocab statement. Error: {}", e);
                                parse_fails += vocab_len;
                            }
                        }

                        println!("Updated Resources: {}", rad_len + kanji_len + vocab_len + kana_vocab.len());
                        if parse_fails > 0 {
                            println!("Parse Failures: {}", parse_fails);
                        }

                        if let Some(tag) = headers.get(reqwest::header::LAST_MODIFIED)
                        {
                            if let Ok(t) = tag.to_str() {
                                conn
                                    .execute("update cache_info set last_modified = ?1, updated_after = ?2, etag = ?3 where id = ?4;", params![t, &request_time.to_rfc3339(), Option::<String>::None, "0"])
                                    .unwrap();
                            }
                        }
                    },
                    _ => {
                        println!("Unexpected data returned while updating resources cache: {:?}", wr.data)
                    },
                }
            }

            Err(s) => println!("{}", s),
        }
    }

    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            sync(&args, &c, ignore_cache);
            c.close().unwrap();
        },
    };
}

fn command_init(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            match setup_db(c) {
                Ok(_) => {},
                Err(e) => {
                    println!("Error setting up SQLite DB: {}", e.to_string())
                },
            }
        },
    };
}

fn setup_db(c: Connection) -> Result<(), SqlError> {
    // Arrays of non-id'ed objects will be stored as json
    // Arrays of ints will be stored as json "[1,2,3]"
    
    // CacheInfo
    c.execute(
        "create table if not exists cache_info (
            id integer primary key,
            etag text,
            last_modified text,
            updated_after text
        )", [])?;

    c.execute("replace into cache_info (id) values (0)", [])?;

    c.execute(wanisql::CREATE_RADICALS_TBL, [])?;
    c.execute(wanisql::CREATE_KANJI_TBL, [])?;
    c.execute(wanisql::CREATE_VOCAB_TBL, [])?;
    c.execute(wanisql::CREATE_KANA_VOCAB_TBL, [])?;

    match c.close() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.1),
    }
}

fn command_test_subject(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e);
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

    match parse_response(response) {
        Ok(t) => test_handle_wani_resp(t.0),
        Err(s) => println!("{}", s),
    }
}

fn parse_response(response: Result<reqwest::blocking::Response, reqwest::Error>) -> Result<(WaniResp, reqwest::header::HeaderMap), String> {
    match response {
        Err(s) => {
            Err(format!("Error with request: {}", s))
        },

        Ok(r) => {
            match r.status() {
                StatusCode::OK => {
                    let headers = r.headers().to_owned();
                    let wani = r.json::<WaniResp>();
                    match wani {
                        Err(s) => Err(format!("Error parsing HTTP 200 response: {}", s)),
                        Ok(w) => {
                            Ok((w, headers))
                        },
                    }
                },
                StatusCode::NOT_MODIFIED => {
                    Ok((WaniResp {
                        url: r.url().to_string(),
                        data_updated_at: None,
                        data: WaniData::Collection(wanidata::Collection { data: vec![] }),
                    }, r.headers().to_owned()))
                },
                StatusCode::UNAUTHORIZED => {
                    Err(format!("HTTP 401: Unauthorized. Make sure your wanikani auth token is correct, and hasn't been expired."))
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    Err(format!("Wanikani API rate limit exceeded."))
                },
                _ => { Err(format!("HTTP status code {}", r.status())) },
            }
        },
    }
}

fn command_summary(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();
    let client = Client::new();
    let response = client
        .get("https://api.wanikani.com/v2/summary")
        .header("Wanikani-Revision", "20170710")
        .bearer_auth(web_config.auth)
        .send();

    match parse_response(response) {
        Ok(wr) => test_handle_wani_resp(wr.0),
        Err(s) => println!("{}", s),
    }
}

fn test_handle_wani_resp(w: WaniResp) -> () {
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

fn setup_connection(args: &Args) -> Result<Connection, WaniError> {
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
                return Err(WaniError::Generic("Could not find home directory. Please manually specify datapath arg. Use \"wani -help\" for more details.".into()));
            }
        }
    }
    
    if !Path::exists(&datapath)
    {
        if let Err(s) = fs::create_dir(&datapath) {
            return Err(WaniError::Generic(format!("Could not create datapath at {}\nError: {}", datapath.display(), s)));
        }
    }

    let mut db_path = PathBuf::from(&datapath);
    db_path.push("wani_cache.db");

    match Connection::open(&db_path) {
        Ok(c) => Ok(c),
        Err(e) => Err(WaniError::Generic(format!("{}", e))),
    }
}

fn get_web_config(args: &Args) -> Result<WaniWebConfig, WaniError> {
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
                        return Err(WaniError::Generic(format!("Could not create wani config folder at {}\nError: {}", configpath.display(), s)));
                    }
                }
                configpath.push(".wani.conf");
            },
            None => {
                return Err(WaniError::Generic(format!("Could not find home directory. Please manually specify configpath arg. Use \"wani -help\" for more details.")));
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
        return Err(WaniError::Generic(format!("Need to specify a wanikani access token")));
    }

    return Ok(WaniWebConfig { auth });
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
