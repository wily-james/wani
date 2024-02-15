mod wanidata;
mod wanisql;

use crate::wanidata::{Assignment, NewReview, PronunciationAudio, ReviewStatus, Subject, SubjectType, WaniData, WaniResp};
use std::cmp::min;
use std::collections::HashMap;
use std::io::BufReader;
use std::io::Write;
use std::ops::Deref;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use wanidata::WaniFmtArgs;
use std::sync::{Arc, PoisonError}; use std::{fmt::Display, fs::{self, File}, io::{self, BufRead}, path::Path, path::PathBuf};
use chrono::DateTime;
use clap::{Parser, Subcommand};
use chrono::Utc;
use itertools::Itertools;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use reqwest::{
    Response, Client, StatusCode
};
use resvg::usvg::{self, Tree};
use rgb::FromSlice;
use rodio::{Decoder, OutputStream, Sink};
use rusqlite::params;
use rusqlite::{
    Connection, Error as SqlError
};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::join;
use tokio::task::JoinSet;
use tokio_rusqlite::Connection as AsyncConnection;
use console:: {
    pad_str, style, Emoji, Term
};
use usvg::{PostProcessingSteps, TreeParsing};
use wana_kana::ConvertJapanese;
use image2ascii::image2ascii;
use wanidata::RateLimit;

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
    QueryReviews,
    ClearReviews,
    TestSubject,
    TestThrottle,
}

/// Info saved to program config file
struct ProgramConfig {
    auth: Option<String>,
    //configpath: PathBuf,
    colorblind: bool,
}

/// Info needed to make WaniKani web requests
struct WaniWebConfig {
    client: Client,
    auth: String,
    revision: String,
}

impl Clone for WaniWebConfig {
    fn clone(&self) -> Self {
        WaniWebConfig {
            client: self.client.clone(),
            auth: self.auth.clone(),
            revision: self.revision.clone(),
        }
    }
}


#[derive(Error, Debug)]
enum WaniError {
    Generic(String),
    Parse(#[from] serde_json::Error),
    Sql(#[from] SqlError),
    AsyncSql(#[from] tokio_rusqlite::Error),
    Chrono(#[from] chrono::ParseError),
    Poison,
    JoinError(#[from] tokio::task::JoinError),
    Io(#[from] std::io::Error),
    //Audio,
    Reqwest(#[from] reqwest::Error),
    Usvg(#[from] usvg::Error),
    RateLimit(Option<wanidata::RateLimit>),
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
            WaniError::Io(e) => e.fmt(f),
            //WaniError::Audio => f.write_str("Audio Playback Error."),
            WaniError::Reqwest(e) => e.fmt(f),
            WaniError::Usvg(e) => e.fmt(f),
            WaniError::RateLimit(r) => {
                match r {
                    Some(r) => f.write_str(&format!("Rate Limit Exceeded Error: {:?}", r)),
                    None => f.write_str("Rate limit error. could not parse rate limit info."),
                }
            },
        }
    }
}

struct SyncResult {
    success_count: usize,
    fail_count: usize,
}

struct AudioMessage {
    send_time: std::time::Instant,
    id: i32,
    audios: Vec<wanidata::PronunciationAudio>, // TODO - we can trim the fat off this message
}

type RateLimitBox = Arc<Mutex<Option<RateLimit>>>;

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
                Command::QueryReviews => command_query_reviews(&args),
                Command::ClearReviews => command_clear_reviews(&args),
                Command::TestSubject => command_test_subject(&args).await,
                Command::TestThrottle => command_test_throttle(&args).await,
            };
        },
        None => command_summary(&args).await,
    };

    Ok(())
}

fn command_clear_reviews(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let _ = c.execute(wanisql::CLEAR_REVIEWS, []);
        },
    };
}

fn command_query_reviews(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_REVIEWS).unwrap();
            match stmt.query_map([], |a| wanisql::parse_review(a)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(reviews) => {
                    for r in reviews {
                        println!("{:?}", r);
                    }
                },
                Err(_) => {},
            };
        },
    };
}

fn command_query_assignments(args: &Args) {
    let conn = setup_connection(&args);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut stmt = c.prepare(wanisql::SELECT_AVAILABLE_ASSIGNMENTS).unwrap();
            match stmt.query_map([Utc::now().timestamp()], |a| wanisql::parse_assignment(a)
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
            let args = [];
            let mut stmt = c.prepare(wanisql::SELECT_ALL_VOCAB).unwrap();
            match stmt.query_map(args, |v| wanisql::parse_vocab(v)
                                 .or_else(|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                Ok(vocab) => {
                    for v in vocab {
                        println!("{:?}", v)
                    }
                },
                Err(e) => { println!("{}", e)},
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

// TODO - command to preload audios
fn play_audio(audio_path: &PathBuf) -> Result<(), WaniError> {
    match OutputStream::try_default() {
        Ok(t) => {
            let file_res = File::open(&audio_path);
            if let Err(_) = file_res {
                return Err(WaniError::Generic(format!("Could not open audio file: {}", audio_path.display())));
            }

            let sink = Sink::try_new(&t.1).expect("Sink broke");
            let source = Decoder::new(BufReader::new(file_res.unwrap()));
            match source {
                Ok(s) => {
                    sink.append(s);
                    sink.sleep_until_end();
                    return Ok(())
                },
                Err(e) => {
                    return Err(WaniError::Generic(format!("Error creating decoder. Error: {}", e)));
                }
            }

        },
        Err(e) => {
            return Err(WaniError::Generic(format!("Error opening default output stream. {}", e)));
        }
    }
}

async fn command_review(args: &Args) {
    // TODO - fix these statistics wrt multiple batches
    fn print_review_screen(term: &Term, done: usize, guesses: usize, failed: usize, total_reviews: usize, width: usize, align: console::Alignment, char_lines: &Vec<String>, review_type_text: &str, toast: &Option<&str>, input: &str) -> Result<(), WaniError> {
        term.clear_screen()?;
        let correct_percentage = if guesses == 0 { 100 } else { ((guesses as f64 - failed as f64) / guesses as f64 * 100.0) as i32 };
        term.write_line(pad_str(&format!("{}: {}%, {}: {}, {}: {}", 
                                         Emoji("\u{1F44D}", "Correct"), correct_percentage, 
                                         Emoji("\u{2705}", "Done"), done, 
                                         Emoji("\u{1F4E9}", "Remaining"), total_reviews - done), 
                                width, align, None).deref())?;
        for char_line in char_lines {
            term.write_line(char_line)?;
        }
        term.write_line(pad_str(&format!("{}:", review_type_text), width, align, None).deref())?;
        term.write_line(input)?;
        if let Some(t) = toast {
            term.write_line(pad_str(&format!("{} {}", Emoji("", "-"), t), width, align, None).deref())?;
        }
        Ok(())
    }

    // TODO - save reviews in another thread
    async fn save_reviews(reviews: HashMap<i32, NewReview>, conn: &AsyncConnection, web_config: &WaniWebConfig, rate_limit: &RateLimitBox) -> Result<(), WaniError> {
        let reviews = Arc::new(reviews);
        let rev = reviews.clone();
        conn.call(move |conn| {
            let tx = conn.transaction();
            if let Err(e) = tx {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            }
            let mut tx = tx.unwrap();
            for (_, review) in rev.deref() {
                let _ = 
                    match wanisql::store_review(&review, &mut tx) {
                        Ok(_) => {},
                        Err(e) => println!("Error storing review: {}", e),
                    };
            }
            tx.commit()?;
            Ok(())
        }).await?;

        let mut join_set = JoinSet::new();
        for (_, review) in reviews.deref() {
            if let ReviewStatus::Done = review.status {
                let new_review = wanidata::NewReviewRequest {
                    review: review.clone()
                };

                let info = RequestInfo {
                    url: "https://api.wanikani.com/v2/reviews/",
                    method: RequestMethod::Post,
                    query: None,
                    json: Some(new_review),
                };

               
                let rate_limit = rate_limit.clone();
                let web_config = web_config.clone();
                join_set.spawn(async move {
                    return send_throttled_request(info, rate_limit, web_config).await
                });
            }
        }

        while let Some(response) = join_set.join_next().await {
            if let Ok(response) = response {
                match response {
                    Ok((wani, _)) => {
                        match wani.data {
                            WaniData::Review(r) => {
                                let ass_id = r.data.assignment_id;
                                conn.call(move |conn| {
                                    conn.execute(wanisql::REMOVE_REVIEW, params![ass_id])?;
                                    Ok(())
                                }).await?;

                                if let Some(resources) = wani.resources_updated {
                                    if let Some(assignment) = resources.assignment {
                                        conn.call(move |conn| {
                                            let tx = conn.transaction();
                                            if let Err(e) = tx {
                                                return Err(tokio_rusqlite::Error::Rusqlite(e));
                                            }
                                            let mut tx = tx.unwrap();
                                            match wanisql::store_assignment(assignment.data, &mut tx) {
                                                Ok(_) => {},
                                                Err(e) => println!("Error storing assignment: {}", e),
                                            };
                                            tx.commit()?;
                                            Ok(())
                                        }).await?;
                                    }
                                }
                            },
                            _ => {}
                        }
                    },
                    Err(e) => {
                        println!("Returned unexpected result when saving review. {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn do_reviews(assignments: &mut Vec<Assignment>, subjects: HashMap<i32, Subject>, audio_cache: PathBuf, web_config: &WaniWebConfig, p_config: &ProgramConfig, image_cache: &PathBuf, conn: &AsyncConnection, rate_limit: &RateLimitBox) -> Result<(), WaniError> {
        assignments.reverse();
        let batch_size = min(20, assignments.len());
        let (audio_tx, mut rx) = mpsc::channel::<AudioMessage>(5);
        let audio_web_config = web_config.clone();
        let audio_task = tokio::spawn(async move {
            let audio_cache = audio_cache;
            let mut last_finish_time = std::time::Instant::now();
            while let Some(msg) = rx.recv().await {
                if msg.send_time < last_finish_time {
                    continue;
                }
                let _ = play_audio_for_subj(msg.id, msg.audios, &audio_cache, &audio_web_config).await;
                last_finish_time = std::time::Instant::now();
            }
        });

        let mut review_result = None;
        let total_assignments = assignments.len();
        while assignments.len() > 0 {
            let mut batch = Vec::with_capacity(batch_size);
            assignments.shuffle(&mut thread_rng());
            for i in (assignments.len() - batch_size..assignments.len()).rev() {
                batch.push(assignments.remove(i));
            }

            let mut reviews = HashMap::with_capacity(batch.len());
            let now = Utc::now();
            for nr in batch.iter().map(|a| wanidata::NewReview {
                id: None,
                assignment_id: a.id,
                created_at: now,
                incorrect_meaning_answers: 0,
                incorrect_reading_answers: 0,
                status: wanidata::ReviewStatus::NotStarted,
            }) {
                reviews.insert(nr.assignment_id, nr);
            }

            let res = do_reviews_inner(&subjects, web_config, p_config, image_cache, &mut reviews, &mut batch, total_assignments, &audio_tx, conn).await;
            if let Err(e) = &res {
                match &e {
                    WaniError::Io(err) => {
                        match err.kind() {
                            io::ErrorKind::Interrupted => {
                                save_reviews(reviews, conn, web_config, rate_limit).await?;
                                return Ok(())
                            },
                            _ => {},
                        }
                    },
                    _ => {},
                }
            }

            review_result = Some(res);
            save_reviews(reviews, conn, web_config, rate_limit).await?;
        }

        audio_task.await?;
        review_result.unwrap_or(Ok(()))
    }

    async fn do_reviews_inner(subjects: &HashMap<i32, Subject>, web_config: &WaniWebConfig, p_config: &ProgramConfig, image_cache: &PathBuf, reviews: &mut HashMap<i32, NewReview>, batch: &mut Vec<Assignment>, total_reviews: usize, audio_tx: &Sender<AudioMessage>, connection: &AsyncConnection) -> Result<(), WaniError> {
        enum AnswerColor {
            Green,
            Red,
            Gray
        }
        let term = Term::buffered_stdout();
        let mut done = 0;
        let mut failed = 0;
        let mut guesses = 0;
        let rng = &mut thread_rng();
        let width = 80;
        let text_width = 50;
        let radical_width = 50;
        let align = console::Alignment::Center;
        let correct_msg = if p_config.colorblind { Some("Correct") } else { None };
        let incorrect_msg = if p_config.colorblind { Some("Inorrect") } else { None };
        let blue_tag = format!("\x1b[{}m", 4 + 40);
        let red_tag = format!("\x1b[{}m", 1 + 40);
        let magenta_tag = format!("\x1b[{}m", 5 + 40);
        let cyan_tag = format!("\x1b[{}m", 6 + 40);
        let green_tag = format!("\x1b[{}m", 2 + 40);
        //let gray_tag = format!("\x1b[48;5;{}m", 145);
        let wfmt_args;

        if term.features().colors_supported() {
            wfmt_args = wanidata::WaniFmtArgs {
                radical_args: wanidata::WaniTagArgs {
                    open_tag: &blue_tag,
                    close_tag: "\x1b[0m",
                },
                kanji_args: wanidata::WaniTagArgs {
                    open_tag: &red_tag,
                    close_tag: "\x1b[0m",
                },
                vocab_args: wanidata::WaniTagArgs {
                    open_tag: &magenta_tag,
                    close_tag: "\x1b[0m",
                },
                meaning_args: wanidata::WaniTagArgs {
                    open_tag: &cyan_tag,
                    close_tag: "\x1b[0m",
                },
                reading_args: wanidata::WaniTagArgs {
                    open_tag: &cyan_tag,
                    close_tag: "\x1b[0m",
                },
                ja_args: wanidata::WaniTagArgs {
                    open_tag: &green_tag,
                    close_tag: "\x1b[0m",
                },
            };
        }
        else {
            wfmt_args = wanidata::EMPTY_ARGS;
        }
        let mut input = String::new();
        'subject: loop {
            if batch.is_empty() {
                break 'subject;
            }
            batch.shuffle(rng);
            /*
            let assignment = batch.iter().find_or_last(|a| { 
                let subj = subjects.get(&a.data.subject_id).unwrap();
                if let wanidata::Subject::Radical(_) = subj {
                    return true;
                }
                false
            }).unwrap();
            */
            let assignment = batch.last().unwrap();
            //let subj_id = assignment.data.subject_id;
            let review = reviews.get_mut(&assignment.id).unwrap();
            let subject = subjects.get(&assignment.data.subject_id);
            if let None = subject {
                term.write_line(&format!("Did not find subject with id: {}", assignment.data.subject_id))?;
                break 'subject;
            }
            let subject = subject.unwrap();
            let characters = match subject {
                Subject::Radical(r) => { 
                    let rad_chars;
                    if let Some(c) = &r.data.characters { 
                        rad_chars = vec![c.to_owned()];
                    } else { 
                        let res = get_radical_image(r, image_cache, radical_width, web_config).await;
                        match res {
                            Ok(rl) => {
                                let mut lines = Vec::new();
                                let mut found_non_empty = false;
                                for line in rl {
                                    if let Ok(l) = line {
                                        if found_non_empty || l.chars().any(|c| c != ' ') {
                                            found_non_empty = true;
                                            lines.push(l);
                                        }
                                    }
                                }

                                for i in (0..lines.len()).rev() {
                                    if lines[i].chars().any(|c| c != ' ') {
                                        break;
                                    }
                                    lines.pop();
                                }

                                rad_chars = lines;
                            }
                            Err(e) => {
                                rad_chars = vec!["Error creating radical image".to_owned()];
                                println!("{}", e);
                                term.read_key()?;
                            }
                        }
                    };
                    rad_chars
                },
                Subject::Kanji(k) => vec![k.data.characters.to_owned()],
                Subject::Vocab(v) => vec![v.data.characters.to_owned()],
                Subject::KanaVocab(kv) => vec![kv.data.characters.to_owned()],
            };
            let is_meaning = match subject {
                Subject::Radical(_) => true,
                Subject::Kanji(_) => {
                    match review.status {
                        wanidata::ReviewStatus::NotStarted => rng.gen_bool(0.5),
                        wanidata::ReviewStatus::MeaningDone => false,
                        wanidata::ReviewStatus::ReadingDone => true,
                        wanidata::ReviewStatus::Done => panic!(),
                    }
                },
                Subject::Vocab(_) => {
                    match review.status {
                        wanidata::ReviewStatus::NotStarted => rng.gen_bool(0.5),
                        wanidata::ReviewStatus::MeaningDone => false,
                        wanidata::ReviewStatus::ReadingDone => true,
                        wanidata::ReviewStatus::Done => panic!(),
                    }
                },
                Subject::KanaVocab(_) => true,
            };
            let review_type_text = match subject {
                Subject::Radical(_) => "Radical Name",
                Subject::Kanji(_) => if is_meaning { "Kanji Meaning" } else { "Kanji Reading" },
                Subject::Vocab(_) => if is_meaning { "Vocab Meaning" } else { "Vocab Reading" },
                Subject::KanaVocab(_) => "Vocab Meaning",
            };
            let padded_chars = characters.iter().map(|l| pad_str(l, width, align, None));
            let char_line = padded_chars.map(|pc| match subject {
                Subject::Radical(_) => style(pc).white().on_blue().to_string(),
                Subject::Kanji(_) => style(pc).white().on_red().to_string(),
                _ => style(pc).white().on_magenta().to_string(),
            }).collect_vec();

            let mut toast = None;
            print_review_screen(&term, done, guesses, failed, total_reviews, width, align, &char_line, review_type_text, &toast, "")?;
            term.move_cursor_to(width / 2, 2 + characters.len())?;
            term.flush()?;

            'input: loop {
                input.clear();
                let mut vis_input = &input;
                let mut kana_input = String::new();

                'line_of_input: loop {
                    let char = term.read_key()?;
                    match char {
                        console::Key::Enter => {
                            break 'line_of_input;
                        },
                        console::Key::Backspace => {
                            input.pop();
                        },
                        console::Key::Char(c) => {
                            input.push(c);
                        },
                        _ => {},
                    };

                    kana_input = input.to_kana();
                    vis_input = if is_meaning { &input } else { &kana_input };
                    let input_padded = pad_str(&vis_input, width, align, None);
                    print_review_screen(&term, done, guesses, failed, total_reviews, width, align, &char_line, review_type_text, &toast, &input_padded)?;
                    let input_width = console::measure_text_width(&vis_input);
                    term.move_cursor_to(width / 2 + input_width / 2, 2 + char_line.len())?;
                    term.flush()?;
                }

                if input.is_empty() {
                    continue 'input;
                }

                let guess = vis_input.trim().to_lowercase();
                let answer_result = wanidata::is_correct_answer(subject, &guess, is_meaning, &kana_input);

                // Tuple (retry, toast, answer_color)
                let tuple = match answer_result {
                    wanidata::AnswerResult::BadFormatting => (true, Some("Try again!"), AnswerColor::Gray),
                    wanidata::AnswerResult::KanaWhenMeaning => (true, Some("We want the reading, not the meaning."), AnswerColor::Gray),
                    wanidata::AnswerResult::FuzzyCorrect | wanidata::AnswerResult::Correct => {
                        let mut toast = correct_msg;
                        if let wanidata::AnswerResult::FuzzyCorrect = answer_result {
                            toast = Some("Answer was a bit off. . .");
                        }
                        review.created_at = Utc::now();
                        review.status = match subject {
                            Subject::Radical(_) | Subject::KanaVocab(_) => 
                            {
                                done += 1;
                                if review.incorrect_meaning_answers > 0 || review.incorrect_reading_answers > 0 {
                                    failed += 1;
                                }
                                batch.pop();
                                wanidata::ReviewStatus::Done
                            },
                            Subject::Kanji(_) | Subject::Vocab(_) => {
                                match review.status {
                                    wanidata::ReviewStatus::NotStarted => {
                                        if is_meaning { 
                                            ReviewStatus::MeaningDone
                                        }
                                        else {
                                            ReviewStatus::ReadingDone
                                        }
                                    },
                                    _ => { 
                                        done += 1;
                                        if review.incorrect_meaning_answers > 0 || review.incorrect_reading_answers > 0 {
                                            failed += 1;
                                        }
                                        batch.pop();
                                        ReviewStatus::Done
                                    }
                                }
                            },
                        };
                        (false, toast, AnswerColor::Green)
                    },
                    wanidata::AnswerResult::Incorrect => {
                        failed += 1;
                        if is_meaning {
                            review.incorrect_meaning_answers += 1;
                        }
                        else {
                            review.incorrect_reading_answers += 1;
                        }
                        (false, incorrect_msg, AnswerColor::Red)
                    },
                    wanidata::AnswerResult::MatchesNonAcceptedAnswer => (true, Some("Answer not accepted. Try again"), AnswerColor::Gray),
                };
                toast = tuple.1;

                if !tuple.0 {
                    guesses += 1;
                }

                let input_line = pad_str(&vis_input, width, align, None);
                let input_formatted = match tuple.2 {
                    AnswerColor::Red => {
                        style(input_line.deref()).white().on_red().to_string()
                    },
                    AnswerColor::Green => {
                        style(input_line.deref()).white().on_green().to_string()
                    },
                    AnswerColor::Gray => {
                        style(input_line.deref()).white().on_color256(238).to_string()
                    },
                };

                print_review_screen(&term, done, guesses, failed, total_reviews, width, align, &char_line, review_type_text, &toast, &input_formatted)?;
                let input_width = console::measure_text_width(&vis_input);
                term.move_cursor_to(width / 2 + input_width / 2, 2 + char_line.len())?;
                term.flush()?;

                enum InfoStatus {
                    Hidden,
                    Open(usize),
                }
                let mut info_status = InfoStatus::Hidden;
                'after_input: loop {
                    match term.read_key()? {
                        console::Key::Enter | console::Key::Backspace=> { break 'after_input; },
                        console::Key::Char(c) => {
                            match c {
                                'f' | 'F' => {
                                    if !tuple.0 { // Don't show info if the user isn't finished
                                                  // guessing
                                        info_status = match info_status {
                                            InfoStatus::Hidden => InfoStatus::Open(0),
                                            InfoStatus::Open(_) => InfoStatus::Hidden,
                                        };
                                    }
                                },
                                'n' | 'N' => {
                                    if !tuple.0 { // Don't show info if the user isn't finished
                                                  // guessing
                                        info_status = match info_status {
                                            InfoStatus::Hidden => InfoStatus::Open(0),
                                            InfoStatus::Open(n) => InfoStatus::Open(n + 1),
                                        };
                                    }
                                },
                                'j' | 'J' => {
                                    let mut can_play_audio = !is_meaning && review.incorrect_reading_answers > 0;
                                    can_play_audio = !tuple.0 && can_play_audio || match review.status {
                                        ReviewStatus::Done | ReviewStatus::ReadingDone => {
                                            true
                                        },
                                        _ => false,
                                    };
                                    if can_play_audio {
                                        let (id, audios) = match subject {
                                            Subject::Radical(r) => (r.id, None),
                                            Subject::Kanji(k) => (k.id, None),
                                            Subject::Vocab(d) => (d.id, Some(d.data.pronunciation_audios.clone())),
                                            Subject::KanaVocab(d) => (d.id, Some(d.data.pronunciation_audios.clone())),
                                        };
                                        if let Some(audios) = audios {
                                            let _ = audio_tx.send(AudioMessage {
                                                send_time: std::time::Instant::now(),
                                                id,
                                                audios,
                                            }).await;
                                        }
                                    }
                                },
                                _ => {},
                            }
                        },
                        _ => {},
                    }

                    print_review_screen(&term, done, guesses, failed, total_reviews, width, align, &char_line, review_type_text, &toast, &input_formatted)?;
                    if let InfoStatus::Open(info_status) = info_status {
                        let lines = get_info_lines(&subject, info_status, width, text_width, align, &wfmt_args, is_meaning, connection).await;
                        for line in &lines {
                            term.write_line(&pad_str(line, width, align, None))?;
                        }

                    }

                    term.move_cursor_to(width / 2 + input.len() / 2, 2 + char_line.len())?;
                    term.flush()?;
                }

                if !tuple.0 {
                    break 'input;
                }

                toast = None;
                print_review_screen(&term, done, guesses, failed, total_reviews, width, align, &char_line, review_type_text, &toast, &"")?;
                let input_width = 0;
                term.move_cursor_to(width / 2 + input_width / 2, 2 + char_line.len())?;
                term.flush()?;
            }
        }

        Ok(())
    }

    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();
    let conn = setup_async_connection(args).await;
    let rate_limit = Arc::new(Mutex::new(None));
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            //if let Ok(wc) = web_config {
                //sync_all(&wc, &c, false).await;
            //}
            
            let mut c_infos = get_all_cache_infos(&c, false).await;
            if let Ok(c_infos) = &mut c_infos {
                println!("Syncing assignments. . .");
                let _ = sync_assignments(&c, &web_config, c_infos.remove(&CACHE_TYPE_SUBJECTS).unwrap_or(CacheInfo { id: CACHE_TYPE_SUBJECTS, ..Default::default()})).await;
            }

            let assignments = select_data(wanisql::SELECT_AVAILABLE_ASSIGNMENTS, &c, wanisql::parse_assignment, [Utc::now().timestamp()]).await;
            if let Err(e) = assignments {
                println!("Error loading assignments. Error: {}", e);
                return;
            };
            let mut assignments = assignments.unwrap();
            if assignments.len() == 0 {
                println!("No assignments for now.");
                return;
            }

            let mut r_ids = vec![];
            let mut k_ids = vec![];
            let mut v_ids = vec![];
            let mut kv_ids = vec![];
            for ass in &assignments {
                match ass.data.subject_type {
                    SubjectType::Radical => r_ids.push(ass.data.subject_id),
                    SubjectType::Kanji => k_ids.push(ass.data.subject_id),
                    SubjectType::Vocab => v_ids.push(ass.data.subject_id),
                    SubjectType::KanaVocab => kv_ids.push(ass.data.subject_id),
                }
            }

            let mut subjects_by_id = HashMap::new();

            let radicals = c.call(move |c| { 
                let stmt = c.prepare(&wanisql::select_radicals_by_id(r_ids.len()));
                match stmt {
                    Err(e) => {
                        return Err(tokio_rusqlite::Error::Rusqlite(e));
                    },
                    Ok(mut stmt) => {
                        match stmt.query_map(rusqlite::params_from_iter(r_ids), |r| wanisql::parse_radical(r)
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
            if let Err(e) = radicals {
                println!("Error loading radicals. Error: {}", e);
                return;
            };
            let radicals = radicals.unwrap();
            for s in radicals {
                subjects_by_id.insert(s.id, wanidata::Subject::Radical(s));
            }

            let kanji = c.call(move |c| { 
                let stmt = c.prepare(&wanisql::select_kanji_by_id(k_ids.len()));
                match stmt {
                    Err(e) => {
                        return Err(tokio_rusqlite::Error::Rusqlite(e));
                    },
                    Ok(mut stmt) => {
                        match stmt.query_map(rusqlite::params_from_iter(k_ids.iter()), |r| wanisql::parse_kanji(r)
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
            if let Err(e) = kanji {
                println!("Error loading kanji. Error: {}", e);
                return;
            };
            let kanji = kanji.unwrap();
            for s in kanji {
                subjects_by_id.insert(s.id, wanidata::Subject::Kanji(s));
            }
            
            let vocab = c.call(move |c| { 
                let stmt = c.prepare(&wanisql::select_vocab_by_id(v_ids.len()));
                match stmt {
                    Err(e) => {
                        return Err(tokio_rusqlite::Error::Rusqlite(e));
                    },
                    Ok(mut stmt) => {
                        match stmt.query_map(rusqlite::params_from_iter(v_ids.iter()), |r| wanisql::parse_vocab(r)
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
            if let Err(e) = vocab {
                println!("Error loading vocab. Error: {}", e);
                return;
            };
            let vocab = vocab.unwrap();
            for s in vocab {
                subjects_by_id.insert(s.id, wanidata::Subject::Vocab(s));
            }

            let kana_vocab = c.call(move |c| { 
                let stmt = c.prepare(&wanisql::select_kana_vocab_by_id(kv_ids.len()));
                match stmt {
                    Err(e) => {
                        return Err(tokio_rusqlite::Error::Rusqlite(e));
                    },
                    Ok(mut stmt) => {
                        match stmt.query_map(rusqlite::params_from_iter(kv_ids.iter()), |r| wanisql::parse_kana_vocab(r)
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
            if let Err(e) = kana_vocab {
                println!("Error loading kana_vocab. Error: {}", e);
                return;
            };
            let kana_vocab = kana_vocab.unwrap();
            for s in kana_vocab {
                subjects_by_id.insert(s.id, wanidata::Subject::KanaVocab(s));
            }

            let audio_cache = get_audio_path(&args);
            if let Err(e) = audio_cache {
                println!("{}", e);
                return;
            }

            let image_cache = get_image_cache(&args);
            if let Err(e) = image_cache {
                println!("{}", e);
                return;
            }

            let _ = ctrlc::set_handler(move || {
                println!("\nreceived Ctrl+C!\nSaving reviews...");
            });

            let res = do_reviews(&mut assignments, subjects_by_id, audio_cache.unwrap(), &web_config, &p_config, &image_cache.unwrap(), &c, &rate_limit).await;
            match res {
                Ok(_) => {},
                Err(e) => {println!("{:?}", e)},
            }
        },
    };
}

async fn list_vocab_from_ids(conn: &AsyncConnection, ids: Vec<i32>, label: &str, width: usize, align: console::Alignment) -> Vec<String> {
    let mut lines = vec![];
    match lookup_vocab(conn, ids).await {
        Ok(vocab) => {
            let mut i = 0;
            let vocab_in_line = 6;
            lines.push(pad_str(label, width, align, None).to_string());
            while i < vocab.len() {
                let mut j = 0;
                let mut vocab_line = vec![];
                while i < vocab.len() && j < vocab_in_line {
                    vocab_line.push(&vocab[i].data.characters);
                    i += 1;
                    j += 1;
                }
                lines.push(pad_str(&vocab_line.iter().join(", "), width, align, None).to_string())
            }
        },
        Err(e) => { 
            lines.push(format!("Error looking up vocab. {}", e));
        }
    }
    lines
}

async fn list_kanji_from_ids(conn: &AsyncConnection, ids: Vec<i32>, label: &str, width: usize, align: console::Alignment) -> Vec<String> {
    let mut lines = vec![];
    match lookup_kanji(conn, ids).await {
        Ok(kanji) => {
            let mut i = 0;
            let kanji_in_line = 6;
            lines.push(pad_str(label, width, align, None).to_string());
            while i < kanji.len() {
                let mut j = 0;
                let mut kanji_line = vec![];
                while i < kanji.len() && j < kanji_in_line {
                    kanji_line.push(&kanji[i].data.characters);
                    i += 1;
                    j += 1;
                }
                lines.push(pad_str(&kanji_line.iter().join(", "), width, align, None).to_string())
            }
        },
        Err(e) => { 
            lines.push(format!("Error looking up kanji. {}", e));
        }
    }
    lines
}

async fn get_info_lines(subject: &Subject, info_status: usize, width: usize, text_width: usize, align: console::Alignment, wfmt_args: &WaniFmtArgs<'_>, is_meaning: bool, conn: &AsyncConnection) -> Vec<String> {
    match subject {
        // 0 - radical name, mnemonic, user synonyms, user note
        // 1 - found in kanji
        Subject::Radical(r) => {
            match info_status % 2 {
                0 => {
                    let mut lines = vec![];
                    let meanings = r.primary_meanings()
                        .join(", ");
                    if meanings.len() > 0 {
                        lines.push(pad_str(&meanings, width, align, None).to_string());
                    }
                    else {
                        lines.push(pad_str("Radical name not found.", width, align, None).to_string())
                    }
                    let mnemonic = wanidata::format_wani_text(&r.data.meaning_mnemonic, wfmt_args);
                    lines.push("---".to_owned());
                    split_str_by_len(&mnemonic, text_width, &mut lines);
                    lines
                },
                1 => {
                    let label = "Found in Kanji:";
                    list_kanji_from_ids(conn, r.data.amalgamation_subject_ids.clone(), label, width, align).await
                },
                _ => { vec![] }
            }
        },

        // 0 - kanji meaning, mnemonic, hint, meaning/reading hint, user note
        // TODO 1 - user synonym, user note
        // 1 - kanji reading, mnemonic, hint
        // 2 - visually similar kanji
        // 3 - found in vocab
        Subject::Kanji(k) => {
            let num_choices = 4;
            let info_status = info_status % num_choices;

            // When we are reviewing "reading":
            // swap the order of the "meaning" and "reading" screens
            let info_status = if is_meaning { info_status } else { 
                match info_status {
                    0 => 1,
                    1 => 0,
                    n => n,
                } 
            };
            match info_status {
                0 => {
                    let mut lines = vec![];
                    let meanings = k.primary_meanings()
                        .join(", ");
                    if meanings.len() > 0 {
                        lines.push(pad_str(&meanings, width, align, None).to_string());
                    }
                    let alt_meanings = k.alt_meanings()
                        .join(", ");
                    if alt_meanings.len() > 0 {
                        lines.push(pad_str(&alt_meanings, width, align, None).to_string());
                    }
                    lines.push("---".to_owned());
                    let mnemonic = wanidata::format_wani_text(&k.data.meaning_mnemonic, &wfmt_args);
                    split_str_by_len(&mnemonic, text_width, &mut lines);
                    lines
                },
                1 => {
                    let mut lines = vec![];
                    let readings = k.primary_readings()
                        .join(", ");
                    if readings.len() > 0 {
                        lines.push(pad_str(&readings, width, align, None).to_string());
                    }
                    let alt_readings = k.alt_readings()
                        .join(", ");
                    if alt_readings.len() > 0 {
                        lines.push(pad_str(&alt_readings, width, align, None).to_string());
                    }
                    lines.push("---".to_owned());
                    let mnemonic = wanidata::format_wani_text(&k.data.reading_mnemonic, &wfmt_args);
                    split_str_by_len(&mnemonic, text_width, &mut lines);
                    lines
                },
                2 => {
                    let label = "Visually Similar Kanji:";
                    list_kanji_from_ids(conn, k.data.visually_similar_subject_ids.clone(), label, width, align).await
                },
                3 => {
                    let label = "Found in Vocab:";
                    list_vocab_from_ids(conn, k.data.amalgamation_subject_ids.clone(), label, width, align).await
                },
                _ => { vec![] }
            }
        },

        // 0 - vocab meaning/reading, mnemonic, user synonym, meaning/reading hint, user note, part
        //   of speech
        // 1 - vocab reading/meaning...
        // 2 - visually similar kanji
        // 3 - found in vocab
        // 4 - Context Pt 1:
        //      - patterns for use
        //      - common word combinations
        // 5 - Context Pt 2:
        //      - context sentences
        // 6 - Kanji composition
        Subject::Vocab(v) => {
            if is_meaning {
                let mut lines = vec![];
                let meanings = v.primary_meanings()
                    .join(", ");
                if meanings.len() > 0 {
                    lines.push(pad_str(&meanings, width, align, None).to_string());
                }
                let alt_meanings = v.alt_meanings()
                    .join(", ");
                if alt_meanings.len() > 0 {
                    lines.push(pad_str(&alt_meanings, width, align, None).to_string());
                }
                lines.push("---".to_owned());
                let mnemonic = wanidata::format_wani_text(&v.data.meaning_mnemonic, &wfmt_args);
                split_str_by_len(&mnemonic, text_width, &mut lines);
                lines
            }
            else {
                let mut lines = vec![];
                let readings = v.primary_readings()
                    .join(", ");
                if readings.len() > 0 {
                    lines.push(pad_str(&readings, width, align, None).to_string());
                }
                let alt_readings = v.alt_readings()
                    .join(", ");
                if alt_readings.len() > 0 {
                    lines.push(pad_str(&alt_readings, width, align, None).to_string());
                }
                lines.push("---".to_owned());
                let mnemonic = wanidata::format_wani_text(&v.data.reading_mnemonic, &wfmt_args);
                split_str_by_len(&mnemonic, text_width, &mut lines);
                lines
            }
        },
        Subject::KanaVocab(kv) => {
            let mut lines = vec![];
            let meanings = kv.primary_meanings()
                .join(", ");
            if meanings.len() > 0 {
                lines.push(pad_str(&meanings, width, align, None).to_string());
            }
            let alt_meanings = kv.alt_meanings()
                .join(", ");
            if alt_meanings.len() > 0 {
                lines.push(pad_str(&alt_meanings, width, align, None).to_string());
            }
            lines.push("---".to_owned());
            let mnemonic = wanidata::format_wani_text(&kv.data.meaning_mnemonic, &wfmt_args);
            split_str_by_len(&mnemonic, text_width, &mut lines);
            lines
        },
    }
}

async fn lookup_vocab(conn: &AsyncConnection, ids: Vec<i32>) -> Result<Vec<wanidata::Vocab>, WaniError> {
    Ok(conn.call(move |c| { 
        let stmt = c.prepare(&wanisql::select_vocab_by_id(ids.len()));
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| wanisql::parse_vocab(r)
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
    }).await?)
}

async fn lookup_kanji(conn: &AsyncConnection, ids: Vec<i32>) -> Result<Vec<wanidata::Kanji>, WaniError> {

    Ok(conn.call(move |c| { 
        let stmt = c.prepare(&wanisql::select_kanji_by_id(ids.len()));
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| wanisql::parse_kanji(r)
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
    }).await?)
}

async fn try_download_text<F>(url: &str, web_config: &WaniWebConfig, path: &PathBuf, modify_content: F) -> Result<(), WaniError> 
where F: Fn(&str) -> String {
    let request = web_config.client
        .get(url);
        //.bearer_auth(&web_config.auth);

    match request.send().await {
        Err(_) => {
            Err(WaniError::Generic(format!("Error fetching file from url: {}", url)))
        },
        Ok(request) => {
            if request.status() != reqwest::StatusCode::OK {
                Err(WaniError::Generic(format!("Error fetching file. HTTP {}", request.status())))
            }
            else {
                if let Ok(mut f) = tokio::fs::File::create(&path).await {
                    let mut body = request.text().await?;
                    body = modify_content(&body);
                    let _ = tokio::io::copy(&mut body.as_bytes(), &mut f).await?;
                    Ok(())
                }
                else {
                    Err(WaniError::Generic("Error opening file to save downloaded content.".into()))
                }
            }
        },
    }
}

async fn try_download_file(url: &str, web_config: &WaniWebConfig, path: &PathBuf) -> Result<(), WaniError> {
    let request = web_config.client
        .get(url);

    match request.send().await {
        Err(_) => {
            Err(WaniError::Generic(format!("Error fetching file from url: {}", url)))
        },
        Ok(request) => {
            if request.status() != reqwest::StatusCode::OK {
                Err(WaniError::Generic(format!("Error fetching file. HTTP {}", request.status())))
            }
            else {
                if let Ok(f) = tokio::fs::File::create(&path).await {
                    let mut reader = tokio::io::BufWriter::new(f);
                    let res = reader.write_all_buf(&mut request.bytes().await?).await;
                    if let Err(e) = res {
                        Err(WaniError::Generic(format!("Error downloading file. {}", e)))
                    }
                    else {
                        Ok(())
                    }
                }
                else {
                    Err(WaniError::Generic("Error opening file to save downloaded content.".into()))
                }
            }
        },
    }
}

async fn get_radical_image(radical: &wanidata::Radical, image_cache: &PathBuf, target_width: u32, web_config: &WaniWebConfig) -> Result<io::Lines<io::BufReader<File>>, WaniError> {
    fn try_convert_image_png(path: &PathBuf, output_path: &PathBuf) -> Result<(), WaniError> {
        let svg = fs::read_to_string(path)?;
        let options = usvg::Options::default();
        let mut tree = Tree::from_str(&svg, &options)?; 
        usvg::TreePostProc::postprocess(&mut tree, PostProcessingSteps::default(), &usvg::fontdb::Database::new());
        let size = tree.size.to_int_size();
        let pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height());
        match pixmap {
            Some(mut pixmap) => {
                let _ = resvg::render(&tree, usvg::Transform::from_scale(1.0, 1.0), &mut pixmap.as_mut());
                for pixel in pixmap.data_mut().as_rgba_mut() {
                    if pixel.a == 0 {
                        pixel.r = 255;
                        pixel.g = 255;
                        pixel.b = 255;
                    }
                }

                if let Err(e) = pixmap.save_png(output_path) {
                    return Err(WaniError::Generic(format!("{}", e)));
                }
                Ok(())
            },
            None => {
                Err(WaniError::Generic("Could not save to png".to_owned()))
            }
        }
    }

    fn try_asciify_image(path: &PathBuf, target_width: u32, text_path: &PathBuf) -> Result<(), WaniError> {
        if let Some(p) = path.to_str() {
            let res = image2ascii(p, target_width, Some(50.0), None);
            match res {
                Ok(a) => {
                    let mut file = fs::File::create(text_path)?;
                    for line in a.to_lines() {
                        writeln!(file, "{}", line)?;
                    }
                    Ok(())
                },
                Err(e) => {
                    Err(WaniError::Generic(format!("{}", e)))
                }
            }
        }
        else {
            Err(WaniError::Generic("Couldn't convert path to string".into()))
        }
    }

    if let Some(image_path) = image_cache.to_str() {
        if let Ok(entries) = glob::glob(&format!("{}/{}_*.txt", image_path, radical.id))
        {
            for entry in entries {
                if let Ok(path) = entry {
                    let txt_path = path.to_str();
                    if let Some(txt_path) = txt_path {
                        return Ok(read_lines(&txt_path)?)
                    }
                }
            }
        }
    }

    let image_names = radical.data.character_images.iter()
        .enumerate()
        .map(|(i, _)| format!("{}_{}", radical.id, i))
        .collect::<Vec<_>>();

    let svg_paths = image_names.iter()
        .enumerate()
        .map(|(_, name)| {
            let mut path = image_cache.clone();
            path.push(format!("{}{}", name, ".svg"));
            path
        })
    .collect::<Vec<_>>();

    let png_paths = image_names.iter()
        .enumerate()
        .map(|(_, name)| {
            let mut path = image_cache.clone();
            path.push(format!("{}{}", name, ".png"));
            path
        })
    .collect::<Vec<_>>();

    let txt_paths = image_names.iter()
        .enumerate()
        .map(|(_, name)| {
            let mut path = image_cache.clone();
            path.push(format!("{}{}", name, ".txt"));
            path
        })
    .collect::<Vec<_>>();

    for i in 0..png_paths.len() {
        let res = try_asciify_image(&png_paths[i], target_width, &txt_paths[i]);
        if let Ok(_) = res {
            return Ok(read_lines(&txt_paths[i])?)
        }
    }

    for i in 0..svg_paths.len() {
        let res = try_convert_image_png(&svg_paths[i], &png_paths[i]);
        if let Ok(_) = res {
            let res = try_asciify_image(&png_paths[i], target_width, &txt_paths[i]);
            if let Ok(_) = res {
                return Ok(read_lines(&txt_paths[i])?)
            }
        }

        match &radical.data.character_images[i].content_type {
            Some(ct) => {
                if ct != "image/svg+xml" {
                    continue;
                }
            }
            None => continue,
        }

        let f = |body: &str| {
            body.replace("var(--color-text, #000)", "rgb(0,0,0)")
        };
        let res = try_download_text(&radical.data.character_images[i].url, web_config, &svg_paths[i], f).await;
        if let Ok(_) = res {
            let res = try_convert_image_png(&svg_paths[i], &png_paths[i]);
            if let Ok(_) = res {
                let res = try_asciify_image(&png_paths[i], target_width, &txt_paths[i]);
                if let Ok(_) = res {
                    return Ok(read_lines(&txt_paths[i])?)
                }
            }
        }
    }

    Err(WaniError::Generic("Failed to convert any images.".into()))
}

async fn play_audio_for_subj(id: i32, audios: Vec<PronunciationAudio>, audio_cache: &PathBuf, web_config: &WaniWebConfig) -> Result<(), WaniError> {
    fn get_audio_path(audio: &PronunciationAudio, audio_cache: &PathBuf, id: i32, index: usize) -> Option<PathBuf> {
        let ext;
        const MPEG: &str = "audio/mpeg";
        const OGG: &str = "audio/ogg";
        const WEBM: &str = "audio/webm";
        if audio.content_type == MPEG {
            ext = Some(".mpeg");
        }
        else if audio.content_type == OGG {
            ext = Some(".ogg");
        }
        else if audio.content_type == WEBM {
            ext = Some(".webm");
        }
        else {
            ext = None;
        }

        if let None = ext {
            return None;
        }
        let ext = ext.unwrap();

        let mut audio_path = audio_cache.clone();
        audio_path.push(format!("{}_{}{}", id, index, ext));
        Some(audio_path)
    }

    let audio_paths = audios.iter()
        .enumerate()
        .map(|(i, a)| get_audio_path(a, audio_cache, id, i))
        .collect::<Vec<_>>();

    for i in 0..audio_paths.len() {
        if let Some(path) = &audio_paths[i] {
            let res = play_audio(&path);
            if let Ok(_) = res {
                return Ok(());
            }
        }
    }

    for i in 0..audios.len() {
        if let Some(path) = &audio_paths[i] {
            let res = try_download_file(&audios[i].url, web_config, &path).await;
            if let Ok(_) = res {
                let play_res = play_audio(&path);
                if let Ok(_) = play_res {
                    return Ok(());
                }
            }
        }
    }

    return Ok(());
}

fn split_str_by_len(s: &str, l: usize, v: &mut Vec<String>)  {
    let mut curr = vec![];
    let mut curr_len = 0;

    for word in s.split_whitespace() {
        let this_len = word.chars().count();
        if curr_len + this_len > l {
            v.push(curr.join(" ").to_string());
            curr = vec![];
            curr_len = 0;
        }

        curr.push(word);
        curr_len += this_len;
    }

    if curr_len > 0 {
        v.push(curr.join(" ").to_string());
    }
}

async fn select_data<T, F, P>(sql: &'static str, c: &AsyncConnection, parse_fn: F, params: P) -> Result<Vec<T>, tokio_rusqlite::Error> 
where T: Send + Sync + 'static, F : Send + Sync + 'static + Fn(&rusqlite::Row<'_>) -> Result<T, WaniError>, P: Send + Sync + 'static + rusqlite::Params {
    return c.call(move |c| { 
        let stmt = c.prepare(sql);
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map(params, |r| parse_fn(r)
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
    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
    if let Err(_) = web_config {
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
                return Err(e);
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

async fn sync_all(web_config: &WaniWebConfig, conn: &AsyncConnection, ignore_cache: bool) {
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

                                tx.commit()?;

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

    c.execute(wanisql::CREATE_REVIEWS_TBL, [])?;
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
async fn command_test_throttle(args: &Args) {
    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }

    let web_config = web_config.unwrap();

    let now = Utc::now().to_rfc3339();
    let rate_limit = Arc::new(Mutex::new(Some(RateLimit {
        limit: 60,
        remaining: 1,
        reset: u64::try_from(Utc::now().timestamp() + 3).unwrap(),
    })));

    println!("{:?}", rate_limit);

    for i in 0..150 {
        let info = RequestInfo {
            url: "https://api.wanikani.com/v2/assignments",
            method: RequestMethod::Get,
            query: Some(vec![("updated_after", &now)]),
            json: None::<bool>,
        };

        let rate_limit_local = rate_limit.clone();
        let web_config = web_config.clone();
        let _ = send_throttled_request(info, rate_limit_local, web_config).await;
        println!("{} - {:?}", i, rate_limit.lock().await.deref());
    }

    println!("All Done!");
}

async fn command_test_subject(args: &Args) {
    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }

    let web_config = web_config.unwrap();
    let query = &[("levels", "5")];
    let response = web_config.client
        .get("https://api.wanikani.com/v2/subjects")
        .header("Wanikani-Revision", &web_config.revision)
        .query(query)
        .bearer_auth(web_config.auth)
        .send();

    match parse_response(response.await).await {
        Ok(t) => test_handle_wani_resp(t.0),
        Err(s) => println!("{}", s),
    }
}

enum RequestMethod {
    Get,
    Post,
}

struct RequestInfo<'a, T: serde::Serialize + Sized> {
    url: &'a str,
    method: RequestMethod,
    query: Option<Vec<(&'a str, &'a str)>>,
    json: Option<T>,
}

fn build_request<'a, T: serde::Serialize + Sized>(info: &RequestInfo<'a, T>, web_config: &WaniWebConfig) -> reqwest::RequestBuilder {
    let request = match info.method {
        RequestMethod::Get => web_config.client.get(info.url),
        RequestMethod::Post => web_config.client.post(info.url),
    };

    let mut request = request 
        .header("Wanikani-Revision", &web_config.revision)
        .bearer_auth(&web_config.auth);

    if let Some(queries) = &info.query {
        if queries.len() > 0 {
            request = request.query(&queries);
        }
    }

    if let Some(json) = &info.json {
        request = request.json(json);
    }

    request
}

async fn send_throttled_request<'a, T: serde::Serialize + Sized>(info: RequestInfo<'a, T>, rate_limit: RateLimitBox, web_config: WaniWebConfig) -> Result<(WaniResp, reqwest::header::HeaderMap), WaniError> {
    loop {
        'wait: loop {
            if let Some(rl) = rate_limit.deref().lock().await.deref() {
                if rl.remaining == 0 {
                    let diff;
                    let now = Utc::now().timestamp();
                    if let Ok(n) = u64::try_from(now) {
                        if rl.reset <= n {
                            println!("Reset reached. No longer waiting.");
                            break 'wait;
                        }

                        diff = rl.reset - n;
                    }
                    else {
                        break 'wait;
                    }

                    println!("Waiting for {} secs.", diff);
                    tokio::time::sleep(std::time::Duration::from_secs(diff)).await;
                }
                else {
                    break 'wait;
                }
            }
            else {
                break 'wait;
            }
        }

        let request = build_request(&info, &web_config);
        let res = parse_response(request.send().await).await;
        match res {
            Ok((wani, headers, new_rl)) => {
                // Update with newest rate-limit
                match new_rl {
                    Some(new_rl) => {
                        let mut rate_limit = rate_limit.deref().lock().await;
                        match rate_limit.deref() {
                            Some(old_rl) => {
                                if old_rl.reset < new_rl.reset {
                                    *rate_limit = Some(new_rl);
                                }
                            },
                            None => {
                                *rate_limit = Some(new_rl);
                            }
                        }
                    },
                    None => {
                        *rate_limit.deref().lock().await = None;
                    },
                }

                return Ok((wani, headers))
            },
            Err(e) => {
                match e {
                    WaniError::RateLimit(new_rl) => {
                        // Update with newest rate-limit
                        match new_rl {
                            Some(new_rl) => {
                                let mut rate_limit = rate_limit.deref().lock().await;
                                match rate_limit.deref() {
                                    Some(old_rl) => {
                                        if old_rl.reset < new_rl.reset {
                                            *rate_limit = Some(new_rl);
                                        }
                                    },
                                    None => {
                                        *rate_limit = Some(new_rl);
                                    }
                                }
                            },
                            None => {
                                *rate_limit.deref().lock().await = None;
                            },
                        }
                    }
                    _ => return Err(e),
                }
            }
        }
    }
}

async fn parse_response(response: Result<Response, reqwest::Error>) -> Result<(WaniResp, reqwest::header::HeaderMap, Option<wanidata::RateLimit>), WaniError> {
    match response {
        Err(s) => {
            Err(WaniError::Generic(format!("Error with request: {}", s)))
        },

        Ok(r) => {
            match r.status() {
                StatusCode::OK => {
                    let headers = r.headers().to_owned();
                    let ratelimit = wanidata::RateLimit::from(&headers);
                    let wani = r.json::<WaniResp>().await;
                    match wani {
                        Err(s) => Err(WaniError::Generic(format!("Error parsing HTTP 200 response: {}", s))),
                        Ok(w) => {
                            Ok((w, headers, ratelimit))
                        },
                    }
                },
                StatusCode::CREATED => {
                    let headers = r.headers().to_owned();
                    let ratelimit = wanidata::RateLimit::from(&headers);
                    let wani = r.json::<WaniResp>().await;
                    match wani {
                        Err(s) => Err(WaniError::Generic(format!("Error parsing HTTP 201 response: {}", s))),
                        Ok(w) => {
                            Ok((w, headers, ratelimit))
                        },
                    }
                },
                StatusCode::NOT_MODIFIED => {
                    let headers = r.headers().to_owned();
                    let ratelimit = wanidata::RateLimit::from(&headers);
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
                        resources_updated: None,
                    }, headers, ratelimit))
                },
                StatusCode::UNAUTHORIZED => {
                    Err(WaniError::Generic(format!("HTTP 401: Unauthorized. Make sure your wanikani auth token is correct, and hasn't been expired.")))
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    println!("Rate limit hit");
                    let limit = wanidata::RateLimit::from(r.headers());
                    if let None = limit {
                        println!("Expected rate limit but none hit");
                    }
                    Err(WaniError::RateLimit(wanidata::RateLimit::from(r.headers())))
                },
                StatusCode::UNPROCESSABLE_ENTITY => {
                    Err(WaniError::Generic(format!("Unprocessable Enitity. {}", r.text().await.unwrap_or("Unprocessable Entity.".to_owned()))))
                },
                _ => { Err(WaniError::Generic(format!("HTTP status code {}", r.status()))) },
            }
        },
    }
}

async fn command_summary(args: &Args) {
    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
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

fn get_image_cache(args: &Args) -> Result<PathBuf, WaniError> {
    let mut db_path = get_db_path(args)?;
    db_path.pop();
    db_path.push("images");
    
    if !Path::exists(&db_path)
    {
        if let Err(s) = fs::create_dir(&db_path) {
            return Err(WaniError::Generic(format!("Could not create image cache path at {}\nError: {}", db_path.display(), s)));
        }
    }

    return Ok(db_path);
}

fn get_audio_path(args: &Args) -> Result<PathBuf, WaniError> {
    let mut db_path = get_db_path(args)?;
    db_path.pop();
    db_path.push("audio");
    
    if !Path::exists(&db_path)
    {
        if let Err(s) = fs::create_dir(&db_path) {
            return Err(WaniError::Generic(format!("Could not create audio cache path at {}\nError: {}", db_path.display(), s)));
        }
    }

    return Ok(db_path);
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

fn get_program_config(args: &Args) -> Result<ProgramConfig, WaniError> {
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

    let mut config = ProgramConfig { 
        auth: None, 
        //configpath: configpath.clone(), 
        colorblind: false,
    };
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
                    "colorblind:" => {
                        config.colorblind = match words[1] {
                            "true" | "True" | "t" => true,
                            _ => false,
                        };
                    },
                    _ => {},
                }
            }
        }
    }
    else {
        return Err(WaniError::Generic(format!("Error reading config at: {}", configpath.display())));
    }

    if let Some(a) = &args.auth {
        config.auth = Some(String::from(a));
    }

    Ok(config)
}

fn get_web_config(config: &ProgramConfig) -> Result<WaniWebConfig, WaniError> {
    if let Some(a) = &config.auth {
        return Ok(WaniWebConfig { 
            client: Client::new(),
            auth: a.into(),
            revision: "20170710".to_owned()
        });
    }
    else {
        return Err(WaniError::Generic(format!("Need to specify a wanikani access token")));
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
