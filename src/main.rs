mod wanidata;

use crate::wanidata::{
    AuxMeaning, CacheInfoSchema, CacheInfoType, WaniData, WaniResp
};
use std::{fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use clap::{Parser, Subcommand};
use chrono::Utc;
use reqwest::{
    blocking::Client, header::HeaderMap, Response, StatusCode
    //Error,
};
use rusqlite::{
    Connection, Error as SqlError, Statement
};

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
    /// Syncs local data with WaniKani servers
    Sync,
    /// Forces update of local data instead of only fetching new data
    ForceSync,
    /// Check the cache info in db
    CacheInfo,
    QueryRadicals,
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
                Command::TestSubject => wani_test_subject(&args),
                Command::Sync => wani_sync(&args, false),
                Command::ForceSync => wani_sync(&args, true),
                Command::CacheInfo => check_cache_info(&args),
                Command::QueryRadicals => wani_radicals(&args),
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

fn wani_radicals(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e.msg),
        Ok(c) => {
            let mut stmt = c.prepare("select 
                                     id,
                                      aux_meanings,
                                      created_at,
                                      document_url,
                                      hidden_at,
                                      lesson_position,
                                      level,
                                      meaning_mnemonic,
                                      meanings,
                                      slug,
                                      srs_id,
                                      amalgamation_subject_ids,
                                      characters,
                                      character_images from radicals;").unwrap();

            match stmt.query_map([], |r| parse_radical(r))
            {
                Ok(radicals) => {
                    for _ in radicals {
                    }
                },
                Err(_) => {},
            };
        },
    };
}

fn check_cache_info(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e.msg),
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

fn store_radical(r: wanidata::Radical, stmt: &mut Statement<'_>) -> Result<usize, SqlError>
{
    let p = rusqlite::params!(
        format!("{}", r.id),
        serde_json::to_string(&r.data.aux_meanings).unwrap(),
        r.data.created_at.to_rfc3339(),
        r.data.document_url,
        if let Some(hidden_at) = r.data.hidden_at { hidden_at.to_rfc3339() } else { "null".into() },
        format!("{}", r.data.lesson_position),
        format!("{}", r.data.level),
        r.data.meaning_mnemonic,
        serde_json::to_string(&r.data.meanings).unwrap(),
        r.data.slug,
        format!("{}", r.data.spaced_repetition_system_id),
        serde_json::to_string(&r.data.amalgamation_subject_ids).unwrap(),
        if let Some(chars) = r.data.characters { chars } else { "null".into() },
        serde_json::to_string(&r.data.character_images).unwrap(),
        );
    return stmt.execute(p);
}

fn parse_radical(r: &rusqlite::Row<'_>) -> Result<wanidata::Radical, Error> {
    println!("{}, {}", r.get::<usize, i32>(0).unwrap(), r.get::<usize, String>(1).unwrap());
    return Ok(wanidata::Radical {
        id: r.get::<usize, i32>(0)?,
        data: wanidata::RadicalData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&r.get::<usize, String>(1)?)?,
            created_at: (), 
            document_url: (), 
            hidden_at: (), 
            lesson_position: (), 
            level: (), 
            meaning_mnemonic: (), 
            meanings: (), 
            slug: (), 
            spaced_repetition_system_id: (), 
            amalgamation_subject_ids: (), 
            characters: (), 
            character_images: () }
    });
}

fn wani_sync(args: &Args, ignore_cache: bool) {
    fn sync(args: &Args, conn: &Connection, ignore_cache: bool) {
        let web_config = get_web_config(&args);
        if let Err(e) = web_config {
            println!("{}", e.msg);
            return;
        }

        let mut etag: Option<String> = None;
        let mut updated_after: Option<String> = None;
        if !ignore_cache {
            let res = conn.query_row("select i.etag, i.updated_after from cache_info i where id = 0;",
                                     [],
                                     |r| Ok((r.get::<usize, Option<String>>(0), r.get::<usize, Option<String>>(1))));
            match res {
                Ok(t) => {
                    if let Ok(tag) = t.0 {
                        etag = tag;
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

        if let Some(tag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, &tag);
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
                        let mut kanji: Vec<String> = vec![];
                        let mut vocab: Vec<String> = vec![];
                        let mut kana_vocab: Vec<String> = vec![];
                        for wd in c.data {
                            match wd {
                                WaniData::Radical(r) => {
                                    radicals.push(r);
                                }, 
                                WaniData::Kanji(k) => {
                                    if let Ok(s) = wanidata::Kanji::to_sql_str(k) {
                                        kanji.push(s);
                                    }
                                    else {
                                        parse_fails += 1;
                                    }
                                },
                                WaniData::Vocabulary(v) => {
                                    if let Ok(s) = wanidata::Vocab::to_sql_str(&v) {
                                        vocab.push(s);
                                    }
                                    else {
                                        parse_fails += 1;
                                    }
                                },
                                WaniData::KanaVocabulary(kv) => {
                                    if let Ok(s) = wanidata::KanaVocab::to_sql_str(&kv) {
                                        kana_vocab.push(s);
                                    }
                                    else {
                                        parse_fails += 1;
                                    }
                                },
                                _ => {},
                            }
                        }

                        let radicals_str = "replace into radicals
                            (id,
                             aux_meanings,
                             created_at,
                             document_url,
                             hidden_at,
                             lesson_position,
                             level,
                             meaning_mnemonic,
                             meanings,
                             slug,
                             srs_id,
                             amalgamation_subject_ids,
                             characters,
                             character_images)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";

                        let mut stmt = conn.prepare(radicals_str).unwrap();
                        let rad_len = radicals.len();
                        for r in radicals {
                            match store_radical(r, &mut stmt) {
                                Err(_) => {
                                    println!("Error inserting into radicals.\n{}", radicals_str);
                                    parse_fails += 1;
                                }
                                Ok(_) => {},
                            }
                        }

                        println!("Updated Resources: {}", rad_len + kanji.len() + vocab.len() + kana_vocab.len());
                        if parse_fails > 0 {
                            println!("Parse Failures: {}", parse_fails);
                        }

                        if let Some(tag) = headers.get(reqwest::header::ETAG)
                        {
                            if let Ok(t) = tag.to_str() {
                                conn
                                    .execute("update cache_info set etag = ?1, updated_after = ?2 where id = ?3;", [t, &request_time.to_rfc3339(), "0"])
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
        Err(e) => println!("{}", e.msg),
        Ok(c) => {
            sync(&args, &c, ignore_cache);
            c.close().unwrap();
        },
    };
}

fn wani_init(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e.msg),
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

    // Radicals
    c.execute(
        "create table if not exists radicals (
            id integer primary key,
            aux_meanings text not null,
            created_at text not null, 
            document_url text not null,
            hidden_at text,
            lesson_position integer not null,
            level integer not null,
            meaning_mnemonic text not null,
            meanings text not null,
            slug text not null,
            srs_id integer not null,
            amalgamation_subject_ids text not null,
            characters text,
            character_images text not null
        )", [])?;
    
    // Kanji
    c.execute(
        "create table if not exists kanji (
            id integer primary key,
            aux_meanings text not null,
            created_at text not null, 
            document_url text not null,
            hidden_at text,
            lesson_position integer not null,
            level integer not null,
            meaning_mnemonic text not null,
            meanings text not null,
            slug text not null,
            srs_id integer not null,
            characters text not null,
            amalgamation_subject_ids text not null,
            component_subject_ids text not null,
            meaning_hint text,
            reading_hint text,
            reading_mnemonic text not null,
            readings text not null,
            visually_similar_subject_ids text
        )", [])?;
    
    // Vocab
    c.execute(
        "create table if not exists vocab (
            id integer primary key,
            aux_meanings text not null,
            created_at text not null, 
            document_url text not null,
            hidden_at text,
            lesson_position integer not null,
            level integer not null,
            meaning_mnemonic text not null,
            meanings text not null,
            slug text not null,
            srs_id integer not null,
            characters text not null,
            component_subject_ids text not null,
            context_sentences text not null,
            parts_of_speech text not null,
            pronunciation_audios text not null,
            readings text not null,
            reading_mnemonic text not null
        )", [])?;
    
    // KanaVocab
    c.execute(
        "create table if not exists kana_vocab (
            id integer primary key,
            aux_meanings text not null,
            created_at text not null, 
            document_url text not null,
            hidden_at text,
            lesson_position integer not null,
            level integer not null,
            meaning_mnemonic text not null,
            meanings text not null,
            slug text not null,
            srs_id integer not null,
            characters text not null,
            context_sentences text not null,
            parts_of_speech text not null,
            pronunciation_audios text not null
        )", [])?;

    match c.close() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.1),
    }
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

    match parse_response(response) {
        Ok(t) => handle_wani_resp(t.0),
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

    match parse_response(response) {
        Ok(wr) => handle_wani_resp(wr.0),
        Err(s) => println!("{}", s),
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
