mod wanidata;
mod wanisql;

use crate::wanidata::WaniData;
use crate::wanidata::WaniResp;
use std::collections::HashMap;
use std::sync::PoisonError;
use std::{fmt::Display, fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use chrono::DateTime;
use clap::{Parser, Subcommand};
use chrono::Utc;
use reqwest::Response;
use reqwest::{
    Client, StatusCode
};
use rusqlite::params;
use rusqlite::{
    Connection, Error as SqlError
};
use thiserror::Error;
use tokio::join;
use tokio_rusqlite::Connection as AsyncConnection;

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
    /// Begin or resume a review session.
    Review,

    // Debug/Testing commands:
    /// Check the cache info in db
    CacheInfo,
    QueryRadicals,
    QueryKanji,
    QueryVocab,
    QueryKanaVocab,
    QueryAssignments,
    TestSubject,
}

/// Info saved to program config file
struct ProgramConfig {
    auth: Option<String>,
}

/// Info needed to make WaniKani web requests
struct WaniWebConfig {
    client: Client,
    auth: String,
    revision: String,
}


#[derive(Error, Debug)]
enum WaniError {
    Generic(String),
    Parse(#[from] serde_json::Error),
    Sql(#[from] SqlError),
    AsyncSql(#[from] tokio_rusqlite::Error),
    Chrono(#[from] chrono::ParseError),
    Poison,
    JoinError(#[from] tokio::task::JoinError)
}

impl<T> From<PoisonError<T>> for WaniError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

impl Display for WaniError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaniError::Generic(g) => f.write_str(g),
            WaniError::Parse(e) => e.fmt(f),
            WaniError::Sql(e) => e.fmt(f),
            WaniError::AsyncSql(e) => e.fmt(f),
            WaniError::Chrono(e) => e.fmt(f),
            WaniError::Poison => f.write_str("Error: Mutex poisoned."),
            WaniError::JoinError(e) => e.fmt(f),
        }
    }
}

// TODO - only pub to silence warning
#[derive(Default)]
pub struct CacheInfo {
    id: usize,
    pub etag: Option<String>,
    last_modified: Option<String>,
    updated_after: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), WaniError> {
    let args = Args::parse();

    match &args.command {
        Some(c) => {
            match c {
                Command::Summary => command_summary(&args).await,
                Command::S => command_summary(&args).await,
                Command::Init => command_init(&args),
                Command::Sync => command_sync(&args, false).await,
                Command::ForceSync => command_sync(&args, true).await,
                Command::Review => command_review(&args).await,

                // Testing
                Command::CacheInfo => command_cache_info(&args),
                Command::QueryRadicals => command_query_radicals(&args),
                Command::QueryKanji => command_query_kanji(&args),
                Command::QueryVocab => command_query_vocab(&args),
                Command::QueryKanaVocab => command_query_kana_vocab(&args),
                Command::QueryAssignments => command_query_assignments(&args),
                Command::TestSubject => command_test_subject(&args).await,
            };
        },
        None => command_summary(&args).await,
    };

    Ok(())
}

fn command_query_assignments(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_AVAILABLE_ASSIGNMENTS).unwrap();
            match stmt.query_map([], |a| wanisql::parse_assignment(a)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(assigns) => {
                    for a in assigns {
                        println!("{:?}", a)
                    }
                },
                Err(_) => {},
            };
        },
    };

}

fn command_query_kana_vocab(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_ALL_KANA_VOCAB).unwrap();
            match stmt.query_map([], |v| wanisql::parse_kana_vocab(v)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(kana_vocab) => {
                    for v in kana_vocab {
                        println!("{:?}", v)
                    }
                },
                Err(_) => {},
            };
        },
    };
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

async fn command_review(args: &Args) {
    let web_config = get_web_config(&args);

    let conn = setup_async_connection(&args).await;
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            if let Ok(wc) = web_config {
                sync_all(&wc, &c, false).await;
            }
            
            let assignments = select_data(wanisql::SELECT_AVAILABLE_ASSIGNMENTS, &c, wanisql::parse_assignment).await;
            if let Err(e) = assignments {
                println!("Error loading assignments. Error: {}", e);
                return;
            };
            let assignments = assignments.unwrap();
            clear_screen();
            println!("Correct: {}%, Done: {}, Remaining: {}", 100, 0, assignments.len());
        },
    };
}

fn clear_screen() {
    print!("{esc}c", esc = 27 as char);
}

async fn select_data<T, F>(sql: &'static str, c: &AsyncConnection, parse_fn: F) -> Result<Vec<T>, tokio_rusqlite::Error> 
where T: Send + Sync + 'static, F : Send + Sync + 'static + Fn(&rusqlite::Row<'_>) -> Result<T, WaniError> {
    return c.call(move |c| { 
        let stmt = c.prepare(sql);
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map([], |r| parse_fn(r)
                                     .or_else
                                     (|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                    Ok(radicals) => {
                        let mut rads = vec![];
                        for r in radicals {
                            if let Ok(rad) = r {
                                rads.push(rad);
                            }
                        }
                        Ok(rads)
                    },
                    Err(e) => {Err(tokio_rusqlite::Error::Rusqlite(e))},
                }
            }
        }
    }).await;
}

fn command_cache_info(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare("select * from cache_info").unwrap();
            match stmt.query_map([], 
                        |r| Ok((r.get::<usize, i32>(0)?, r.get::<usize, Option<String>>(1)?, r.get::<usize, Option<String>>(2)?, r.get::<usize, Option<String>>(3)?))) {
                Ok(infos) => {
                    for info in infos {
                        match info {
                            Ok(t) => {
                                println!("CacheInfo: Type {}, ETag {}, LastMod {}, UpdatedAfter {}", 
                                         t.0, 
                                         t.1.unwrap_or("None".into()), 
                                         t.2.unwrap_or("None".into()), 
                                         t.3.unwrap_or("None".into()));
                            },
                            Err(e) => {
                                println!("Error parsing sql row for cache info. Error {}", e);
                            },
                        }
                    }
                },
                Err(e) => {
                    println!("Error checking cache_info: {}", e);
                }
            };
        },
    };
}

fn get_cache_info(conn: &Connection, cache_type: usize) -> Result<CacheInfo, rusqlite::Error> {
    return conn.query_row("select i.last_modified, i.updated_after, i.etag from cache_info i where id = ?1;",
                             params![cache_type],
                             |r| Ok(CacheInfo {
                                 id: cache_type,
                                 last_modified: r.get::<usize, Option<String>>(0)?, 
                                 updated_after: r.get::<usize, Option<String>>(1)?,
                                 etag: r.get::<usize, Option<String>>(2)? }));
}

async fn get_all_cache_infos(conn: &AsyncConnection, ignore_cache: bool) -> Result<HashMap<usize, CacheInfo>, WaniError> {
    if ignore_cache {
        return Ok(HashMap::new());
    }

    Ok(conn.call(|conn| {
        let mut stmt = conn.prepare("select i.id, i.last_modified, i.updated_after, i.etag from cache_info i;")?;
        let infos = stmt.query_map([],
                                   |r| Ok(CacheInfo {
                                       id: r.get::<usize, usize>(0)?,
                                       last_modified: r.get::<usize, Option<String>>(1)?, 
                                       updated_after: r.get::<usize, Option<String>>(2)?,
                                       etag: r.get::<usize, Option<String>>(3)? }))?;

        let mut map = HashMap::new();
        for info in infos {
            if let Ok(i) = info {
                map.insert(i.id, i);
            }
        }
        return Ok(map);
    }).await?)
}

async fn command_sync(args: &Args, ignore_cache: bool) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        return;
    }
    let web_config = web_config.unwrap();

    let conn = setup_async_connection(&args).await;
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            sync_all(&web_config, &c, ignore_cache).await;
        },
    };
}

async fn sync_all(web_config: &WaniWebConfig, conn: &AsyncConnection, ignore_cache: bool) {
    struct SyncResult {
        success_count: usize,
        fail_count: usize,
    }

    async fn sync_assignments(conn: &AsyncConnection, web_config: &WaniWebConfig, cache_info: CacheInfo) -> Result<SyncResult, WaniError> {
        let mut next_url = Some("https://api.wanikani.com/v2/assignments".to_owned());

        let mut assignments = vec![];
        let mut last_request_time: Option<DateTime<Utc>> = None;
        while let Some(url) = next_url {
            next_url = None;
            let mut request = web_config.client
                .get(url)
                .header("Wanikani-Revision", &web_config.revision)
                .bearer_auth(&web_config.auth);

            if let Some(updated_after) = &cache_info.updated_after {
                request = request.query(&[("updated_after", updated_after)]);
            }

            last_request_time = Some(Utc::now());
            match parse_response(request.send().await).await {
                Ok(t) => {
                    match t.0.data {
                        WaniData::Collection(c) => {
                            next_url = c.pages.next_url;
                            for wd in c.data {
                                match wd {
                                    WaniData::Assignment(a) => {
                                        assignments.push(a);
                                    },
                                    _ => {},
                                }
                            }
                        },
                        _ => {
                            last_request_time = None; // clear last request time to avoid invalidate
                                                      // cache
                            println!("Unexpected response when fetching assignment data. {:?}", t.0.data);
                        },
                    }
                },
                Err(e) => {
                    println!("Failed to fetch current assignment data. Error: {}", e);
                    return Err(WaniError::Generic(e));
                }
            }
        }

        let ass_count = assignments.len();
        let ass_fail = conn.call(|c| {
            let tx = c.transaction();
            if let Err(e) = tx {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            }
            let mut tx = tx.unwrap();
            let mut ass_fail = 0;
            for ass in assignments {
                match wanisql::store_assignment(ass, &mut tx) {
                    Ok(_) => {},
                    Err(_) => ass_fail += 1,
                };
            }
            tx.commit()?;
            Ok(ass_fail)
        }).await?; // Await this before updating cache so we don't update cache if there's a
                   // problem inserting

        if let Some(time) = last_request_time {
            match update_cache(None, CACHE_TYPE_ASSIGNMENTS, time, &conn).await {
                Ok(_) => (),
                Err(e) => { 
                    println!("Failed to update assignment cache. Error: {}", e);
                },
            }
        }

        return Ok(SyncResult {
            success_count: ass_count,
            fail_count: ass_fail,
        });
    }

    async fn sync_subjects(conn: &AsyncConnection, 
                           web_config: &WaniWebConfig, subjects_cache: CacheInfo) -> Result<SyncResult, WaniError> {
        let mut next_url: Option<String> = Some("https://api.wanikani.com/v2/subjects".into());
        let mut total_parse_fails = 0;
        let mut updated_resources = 0;
        let mut headers: Option<reqwest::header::HeaderMap> = None;
        let mut last_request_time = Utc::now();
        while let Some(url) = next_url {
            let mut request = web_config.client
                .get(url)
                .header("Wanikani-Revision", &web_config.revision)
                .bearer_auth(&web_config.auth);

            if let Some(tag) = &subjects_cache.last_modified {
                request = request.header(reqwest::header::LAST_MODIFIED, tag);
            }

            if let Some(after) = &subjects_cache.updated_after {
                request = request.query(&[("updated_after", after)]);
            }

            last_request_time = Utc::now();
            next_url = None;
            let resp = request.send().await;
            match parse_response(resp).await {
                Ok(t) => {
                    let wr = t.0;
                    headers = Some(t.1);

                    match wr.data {
                        WaniData::Collection(c) => {
                            next_url = c.pages.next_url;
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

                            let fut = conn.call(move |conn| {
                                let mut parse_fails = 0;
                                let mut tx = conn.transaction()?;

                                let rad_len = radicals.len();
                                for r in radicals {
                                    match wanisql::store_radical(r, &mut tx) {
                                        Err(_) => {
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }

                                let kanji_len = kanji.len();
                                for k in kanji {
                                    match wanisql::store_kanji(k, &mut tx) {
                                        Err(_) => {
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }

                                let vocab_len = vocab.len();
                                for v in vocab {
                                    match wanisql::store_vocab(v, &mut tx) {
                                        Err(_) => {
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }

                                let kana_vocab_len = kana_vocab.len();
                                for v in kana_vocab {
                                    match wanisql::store_kana_vocab(v, &mut tx) {
                                        Err(_) => {
                                            parse_fails += 1;
                                        }
                                        Ok(_) => {},
                                    }
                                }

                                Ok(SyncResult {
                                    success_count: rad_len + kanji_len + vocab_len + kana_vocab_len - parse_fails,
                                    fail_count: parse_fails,
                                })
                            });
                            let r = fut.await?;
                            updated_resources += r.success_count;
                            total_parse_fails += r.fail_count;
                        },
                        _ => {
                            println!("Unexpected data returned while updating resources cache: {:?}", wr.data)
                        },
                    }
                }
                Err(s) => {
                    headers = None; // clear out headers to skip updating cache_info.last_modified if any
                                    // requests fail.
                    println!("{}", s);
                },
            }
        }

        if let Some(h) = headers {
            if let Some(tag) = h.get(reqwest::header::LAST_MODIFIED) {
                if let Ok(t) = tag.to_str() {
                    update_cache(Some(t), CACHE_TYPE_SUBJECTS, last_request_time, &conn).await?;
                }
                else {
                    update_cache(None, CACHE_TYPE_SUBJECTS, last_request_time, &conn).await?;
                }
            }
        }

        return Ok(SyncResult {
            success_count: updated_resources,
            fail_count: total_parse_fails,
        });
    }

    let c_infos = get_all_cache_infos(&conn, ignore_cache).await;
    if let Err(e) = c_infos {
        println!("Error fetching cache infos. Error: {}", e);
        return;
    }
    let mut c_infos = c_infos.unwrap();

    let subj_future = sync_subjects(&conn, &web_config, c_infos.remove(&CACHE_TYPE_SUBJECTS).unwrap_or(CacheInfo { id: CACHE_TYPE_SUBJECTS, ..Default::default()}));
    let ass_future = sync_assignments(&conn, &web_config, c_infos.remove(&CACHE_TYPE_ASSIGNMENTS).unwrap_or(CacheInfo { id: CACHE_TYPE_ASSIGNMENTS, ..Default::default()}));
    let res = join![subj_future, ass_future];

    match res.0 {
        Ok(sync_res) => {
            println!("Synced Subjects: {}, Errors: {}", sync_res.success_count, sync_res.fail_count);
        },
        Err(e) => {
            println!("Error syncing subjects: {}", e);
        },
    };
    match res.1 {
        Ok(sync_res) => {
            println!("Synced Assignments: {}, Errors: {}", sync_res.success_count, sync_res.fail_count);
        },
        Err(e) => {
            println!("Error syncing assignments: {}", e);
        },
    };
}

async fn update_cache(last_modified: Option<&str>, cache_type: usize, last_request_time: DateTime<Utc>, conn: &AsyncConnection) -> Result<(), tokio_rusqlite::Error> {
    let last_modified = if let Some(lm) = last_modified { Some(lm.to_owned()) } else { None };
    let last_request_time = last_request_time.to_rfc3339();
    return conn.call(move |c| {
        c.execute("update cache_info set last_modified = ?1, updated_after = ?2, etag = ?3 where id = ?4;", params![last_modified, &last_request_time, Option::<String>::None, cache_type])?;
        Ok(())
    }).await;
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

const CACHE_TYPE_SUBJECTS: usize = 0;
const CACHE_TYPE_ASSIGNMENTS: usize = 1;

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

    c.execute("insert or ignore into cache_info (id) values (?1),(?2)", 
              params![
                CACHE_TYPE_SUBJECTS, 
                CACHE_TYPE_ASSIGNMENTS, 
              ])?;

    c.execute(wanisql::CREATE_RADICALS_TBL, [])?;
    c.execute(wanisql::CREATE_KANJI_TBL, [])?;
    c.execute(wanisql::CREATE_VOCAB_TBL, [])?;
    c.execute(wanisql::CREATE_KANA_VOCAB_TBL, [])?;
    c.execute(wanisql::CREATE_ASSIGNMENTS_TBL, [])?;

    match c.close() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.1),
    }
}

async fn command_test_subject(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }

    let web_config = web_config.unwrap();
    let response = web_config.client
        .get("https://api.wanikani.com/v2/subjects")
        .header("Wanikani-Revision", &web_config.revision)
        .query(&[("levels", "5")])
        .bearer_auth(web_config.auth)
        .send();

    match parse_response(response.await).await {
        Ok(t) => test_handle_wani_resp(t.0),
        Err(s) => println!("{}", s),
    }
}

async fn parse_response(response: Result<Response, reqwest::Error>) -> Result<(WaniResp, reqwest::header::HeaderMap), String> {
    match response {
        Err(s) => {
            Err(format!("Error with request: {}", s))
        },

        Ok(r) => {
            match r.status() {
                StatusCode::OK => {
                    let headers = r.headers().to_owned();
                    let wani = r.json::<WaniResp>().await;
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
                        data: WaniData::Collection(wanidata::Collection { 
                            data: vec![],
                            pages: wanidata::PageData {
                                per_page: 0,
                                next_url: None,
                                previous_url: None,
                            },
                        }),
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

async fn command_summary(args: &Args) {
    let web_config = get_web_config(&args);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();
    let response = web_config.client
        .get("https://api.wanikani.com/v2/summary")
        .header("Wanikani-Revision", web_config.revision)
        .bearer_auth(web_config.auth)
        .send();

    match parse_response(response.await).await {
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

fn get_db_path(args: &Args) -> Result<PathBuf, WaniError> {
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
    return Ok(db_path);
}

async fn setup_async_connection(args: &Args) -> Result<AsyncConnection, WaniError> {
    Ok(AsyncConnection::open(&get_db_path(args)?).await?)
}

fn setup_connection(args: &Args) -> Result<Connection, WaniError> {
    match Connection::open(&get_db_path(args)?) {
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

    return Ok(WaniWebConfig { 
        client: Client::new(),
        auth,
        revision: "20170710".to_owned()
    });
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
