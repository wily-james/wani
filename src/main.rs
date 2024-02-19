mod wanidata;
mod wanisql;

use crate::wanidata::{Assignment, NewReview, ReviewStatus, Subject, SubjectType, WaniData, WaniResp};
use std::cmp::min;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufReader;
use std::io::Write;
use std::ops::Deref;
use std::str::FromStr;
use reqwest::header::HeaderValue;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use wanidata::ContextSentence;
use wanidata::WaniFmtArgs;
use wanisql::parse_review;
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
    /// The data path can also be specified in the wani config file with 
    ///     datapath: /some/path
    #[arg(short, long, value_name = "PATH")]
    datapath: Option<PathBuf>,

    /// Specifies the containing file path for .wani.conf config file. Default is ~/.config/wani/
    /// The config file path can also be specified in the WANI_CONFIG_PATH environment variable
    #[arg(short, long, value_name = "FILE")]
    configfile: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Lists a summary of the Lessons and Reviews that are available. This is the default command.
    Summary,
    /// a shorthand for the 'summary' command
    S,
    /// Begin or resume a review session.
    Review,
    /// a shorthand for the 'review' command
    R,
    /// Begin a lesson session
    Lesson,
    /// A shorthand for the 'lesson' command
    L,
    /// Syncs local data with WaniKani servers
    Sync,
    /// Forces update of local data instead of only fetching new data
    ForceSync,
    /// Does first-time initialization
    Init,
}

/// Info saved to program config file
struct ProgramConfig {
    auth: Option<String>,
    data_path: PathBuf,
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
    Connection(),
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
            WaniError::Connection() => f.write_str("Error related to request connection."),
            WaniError::RateLimit(r) => {
                match r {
                    Some(r) => f.write_str(&format!("Rate Limit Exceeded Error: {:?}", r)),
                    None => f.write_str("Rate limit error. could not parse rate limit info."),
                }
            },
        }
    }
}

enum AnswerColor {
    Green,
    Red,
    Gray
}

struct SyncResult {
    success_count: usize,
    fail_count: usize,
}

struct AudioInfo {
    url: String,
    content_type: String,
}

struct AudioMessage {
    send_time: std::time::Instant,
    id: i32,
    audios: Vec<AudioInfo>,
}

type RateLimitBox = Arc<Mutex<Option<RateLimit>>>;

#[derive(Default)]
struct CacheInfo {
    id: usize, // See CACHE_TYPE_* constants
    etag: Option<String>,
    last_modified: Option<String>,
    updated_after: Option<String>,
}

const CACHE_TYPE_SUBJECTS: usize = 0;
const CACHE_TYPE_ASSIGNMENTS: usize = 1;
const CACHE_TYPE_USER: usize = 2;

#[derive(Default)]
struct SubjectCounts {
    radical_count: usize,
    kanji_count: usize,
    vocab_count: usize,
}

enum ReviewType {
    Lesson(SubjectCounts),
    Review(ReviewStats),
}

#[derive(Default)]
struct ReviewStats {
    done: usize,
    failed: usize,
    guesses: usize,
    total_reviews: usize
}

#[derive(Default, Debug)]
struct LoadedReviews {
    invalid_reviews: Vec<NewReview>,
    finished_reviews: Vec<NewReview>,
    in_progress_reviews: Vec<NewReview>,
}

#[derive(Default)]
enum RequestMethod {
    #[default]
    Get,
    Post,
    Put,
}

#[derive(Default)]
struct RequestInfo<'a, T: serde::Serialize + Sized> {
    url: String,
    method: RequestMethod,
    query: Option<Vec<(&'a str, &'a str)>>,
    headers: Option<Vec<(String, String)>>,
    json: Option<T>,
}

#[tokio::main]
async fn main() -> Result<(), WaniError> {
    let args = Args::parse();

    match &args.command {
        Some(c) => {
            match c {
                Command::Summary => command_summary(&args).await,
                Command::S => command_summary(&args).await,
                Command::Init => command_init(&get_program_config(&args)?),
                Command::Sync => command_sync(&args, false).await,
                Command::ForceSync => command_sync(&args, true).await,
                Command::Review => command_review(&args).await,
                Command::R => command_review(&args).await,
                Command::Lesson => command_lesson(&args).await,
                Command::L => command_lesson(&args).await,
            };
        },
        None => command_summary(&args).await,
    };

    Ok(())
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

async fn print_lesson_screen(term: &Term, meaning_line: &Option<String>, rev_type: &ReviewType, subject: &Subject, image_cache: &PathBuf, web_config: &WaniWebConfig) -> Result<(usize, usize, Vec<String>), WaniError> {
    let width = term.size().1;
    let radical_width = u32::from(width * 5 / 8);
    let width = width.into();

    term.clear_screen()?;
    if let ReviewType::Lesson(subj_counts) = rev_type {
        print_lesson_status(subj_counts, term, width)?;
    }

    let char_line = get_chars_for_subj(&subject, image_cache, radical_width, web_config).await?;
    let char_lines = get_chars_for_subj(&subject, image_cache, radical_width, web_config).await?;
    let padded_chars = char_lines.iter().map(|l| pad_str(l, width, console::Alignment::Center, None));
    let char_lines = padded_chars.map(|pc| match subject {
        Subject::Radical(_) => style(pc).white().on_blue().to_string(),
        Subject::Kanji(_) => style(pc).white().on_red().to_string(),
        _ => style(pc).white().on_magenta().to_string(),
    }).collect_vec();
    for line in &char_lines {
        term.write_line(line)?;
    }
    if let Some(line) = meaning_line {
        term.write_line(line)?;
    }

    Ok((width, width * 5 / 8, char_line))
}

async fn print_review_screen<'a>(term: &Term, rev_type: &mut ReviewType, align: console::Alignment, subject: &Subject, review_type_text: &str, toast: &Option<&str>, image_cache: &PathBuf, web_config: &WaniWebConfig, input: &str, color: Option<&AnswerColor>) -> Result<(usize, usize, Vec<String>), WaniError> {
    term.clear_screen()?;
    let (_, width) = term.size();
    let radical_width = u32::from(width * 5 / 8);
    let width: usize = usize::from(width);

    // Top line changes based on review type
    match rev_type {
        ReviewType::Review(stats) => {
            let correct_percentage = if stats.guesses == 0 { 100 } else { ((stats.guesses as f64 - stats.failed as f64) / stats.guesses as f64 * 100.0) as i32 };
            term.write_line(pad_str(&format!("{}: {}%, {}: {}, {}: {}", 
                                             Emoji("\u{1F44D}", "Correct"), correct_percentage, 
                                             Emoji("\u{2705}", "Done"), stats.done, 
                                             Emoji("\u{1F4E9}", "Remaining"), stats.total_reviews - stats.done), 
                                    width, console::Alignment::Right, None).deref())?;
        },

        ReviewType::Lesson(subj_counts) => {
            print_lesson_status(subj_counts, term, width)?;
        },
    }

    let char_lines = get_chars_for_subj(&subject, image_cache, radical_width, web_config).await?;
    let padded_chars = char_lines.iter().map(|l| pad_str(l, width, align, None));
    let char_lines = padded_chars.map(|pc| match subject {
        Subject::Radical(_) => style(pc).white().on_blue().to_string(),
        Subject::Kanji(_) => style(pc).white().on_red().to_string(),
        _ => style(pc).white().on_magenta().to_string(),
    }).collect_vec();
    for char_line in &char_lines {
        term.write_line(char_line)?;
    }
    term.write_line(pad_str(&format!("{}:", review_type_text), width, align, None).deref())?;

    let input_line = pad_str(&input, width, align, None);
    let input_formatted = if let Some(color) = color { match color {
        AnswerColor::Red => {
            style(input_line.deref()).white().on_red().to_string()
        },
        AnswerColor::Green => {
            style(input_line.deref()).white().on_green().to_string()
        },
        AnswerColor::Gray => {
            style(input_line.deref()).white().on_color256(238).to_string()
        },
    } } else { input_line.to_string() };

    term.write_line(&input_formatted)?;
    if let Some(t) = toast {
        term.write_line(pad_str(&format!("{} {}", "-", t), width, align, None).deref())?;
    }

    Ok((width, width * 5 / 8, char_lines))
}

fn print_lesson_status(subj_counts: &SubjectCounts, term: &Term, width: usize) -> Result<(), WaniError> {
    let msg_emoji = Emoji("\u{1F4E9}", " ");
    let line = &format!("R{}{} K{}{} V{}{}", 
                        msg_emoji, subj_counts.radical_count,
                        msg_emoji, subj_counts.kanji_count,
                        msg_emoji, subj_counts.vocab_count);
    term.write_line(pad_str(line, width, console::Alignment::Right, None).deref())?;
    Ok(())
}

async fn save_lessons(reviews: HashMap<i32, NewReview>, rate_limit: &RateLimitBox, web_config: &WaniWebConfig, conn: &AsyncConnection) -> Result<(), WaniError> {
    let reviews = Arc::new(reviews);
    let rev = reviews.clone();
    conn.call(move |conn| {
        let tx = conn.transaction();
        if let Err(e) = tx {
            return Err(tokio_rusqlite::Error::Rusqlite(e));
        }
        let mut tx = tx.unwrap();
        for (_, review) in rev.deref() {
            let _ = tx.execute(wanisql::REMOVE_REVIEW, [review.assignment_id]);
        }
        for (_, review) in rev.deref() {
            let _ = 
                match wanisql::store_review(&review, &mut tx) {
                    Ok(_) => {},
                    Err(e) => println!("Error saving review locally: {}", e),
                };
        }
        tx.commit()?;
        Ok(())
    }).await?;

    save_lessons_to_wanikani(reviews.iter().map(|t| t.1), rate_limit, web_config, conn).await
}

async fn save_lessons_to_wanikani<'a, I>(lessons: I, rate_limit: &RateLimitBox, web_config: &WaniWebConfig, conn: &AsyncConnection) -> Result<(), WaniError> 
where I: Iterator<Item = &'a NewReview> {
    let mut join_set = JoinSet::new();
    let mut saved_assignments = vec![];
    for review in lessons {
        if let ReviewStatus::Done = review.status {
            let started_at = review.created_at.to_rfc3339();
            let url = format!("https://api.wanikani.com/v2/assignments/{}/start", review.assignment_id);
            let info = RequestInfo {
                url,
                method: RequestMethod::Put,
                json: Some(serde_json::json!({
                    "started_at": started_at,
                })),
                ..Default::default()
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
                        WaniData::Assignment(a) => {
                            conn.call(move |conn| {
                                conn.execute(wanisql::REMOVE_REVIEW, params![a.id])?;
                                Ok(())
                            }).await?;
                            saved_assignments.push(a);
                        },
                        _ => {}

                    }
                },
                Err(e) => {
                    println!("{}", e);
                }
            }
        }
    }

    for a in saved_assignments {
        conn.call(move |conn| {
            let tx = conn.transaction();
            if let Err(e) = tx {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            }
            let mut tx = tx.unwrap();
            match wanisql::store_assignment(a, &mut tx) {
                Ok(_) => {},
                Err(e) => println!("Error storing assignment: {}", e),
            };
            tx.commit()?;
            Ok(())
        }).await?;
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
            let _ = tx.execute(wanisql::REMOVE_REVIEW, [review.assignment_id]);
        }
        for (_, review) in rev.deref() {
            let _ = 
                match wanisql::store_review(&review, &mut tx) {
                    Ok(_) => {},
                    Err(e) => println!("Error saving review locally: {}", e),
                };
        }
        tx.commit()?;
        Ok(())
    }).await?;

    save_reviews_to_wanikani(reviews.deref().iter().map(|t| t.1), rate_limit, web_config, conn).await?;
    Ok(())
}

async fn save_reviews_to_wanikani<'a, I>(reviews: I, rate_limit: &RateLimitBox, web_config: &WaniWebConfig, conn: &AsyncConnection) -> Result<Vec<wanidata::Review>, WaniError>
where I: Iterator<Item = &'a NewReview> {
    let mut join_set = JoinSet::new();
    for review in reviews {
        if let ReviewStatus::Done = review.status {
            let new_review = wanidata::NewReviewRequest {
                review: review.clone()
            };

            let info = RequestInfo {
                url: "https://api.wanikani.com/v2/reviews/".to_owned(),
                method: RequestMethod::Post,
                json: Some(new_review),
                query: None,
                headers: None,
            };


            let rate_limit = rate_limit.clone();
            let web_config = web_config.clone();
            join_set.spawn(async move {
                return send_throttled_request(info, rate_limit, web_config).await
            });
        }
    }

    let mut had_connection_issue = false;
    let mut errors = vec![];
    let mut saved_reviews = vec![];
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
                            saved_reviews.push(r);

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
                    match e {
                        WaniError::Connection() => {
                            had_connection_issue = true;
                        }
                        _ => {
                            errors.push(format!("Unable to submit review to WaniKani. {}", e));
                        },
                    }
                }
            }
        }
    }

    if had_connection_issue {
        println!("Unable to submit review to WaniKani due to internet connection issue.");
        println!("Review progress is still saved locally.");
    }

    for e in errors {
        println!("{}", e);
    }

    Ok(saved_reviews)
}

async fn command_lesson(args: &Args) {
    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();

    let rate_limit = Arc::new(Mutex::new(None));
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();

    let conn = setup_async_connection(&p_config).await;
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut ass_cache_info = CacheInfo { id: CACHE_TYPE_SUBJECTS, ..Default::default() };
            let mut c_infos = get_all_cache_infos(&c, false).await;
            if let Ok(c_infos) = &mut c_infos {
                if let Some(info) = c_infos.remove(&CACHE_TYPE_SUBJECTS) {
                    ass_cache_info = info;
                }
            }

            println!("Syncing assignments. . .");
            let is_user_restricted = is_user_restricted(&web_config, &c, &rate_limit).await;
            let _ = sync_assignments(&c, &web_config, ass_cache_info, &rate_limit, is_user_restricted).await;
            let assignments = select_data(wanisql::SELECT_LESSON_ASSIGNMENTS, &c, wanisql::parse_assignment, []).await;
            if let Err(e) = assignments {
                println!("Error loading assignments. Error: {}", e);
                return;
            };
            let assignments = assignments.unwrap();
            if assignments.len() == 0 {
                println!("No assignments for now.");
                return;
            }

            let existing_lessons = load_existing_lessons(&c, &assignments).await;
            let existing_lessons = match existing_lessons {
                Ok(existing_reviews) => { 
                    existing_reviews 
                },
                Err(e) => {
                    println!("Error loading existing lessons: {}", e);
                    LoadedReviews::default()
                },
            };

            for review in existing_lessons.invalid_reviews {
                let _ = c.call(move |conn| {
                    conn.execute(wanisql::REMOVE_REVIEW, params![review.assignment_id])?;
                    Ok(())
                }).await;
            }

            let _ = save_lessons_to_wanikani(existing_lessons.finished_reviews.iter(), &rate_limit, &web_config, &c).await;

            let mut use_assignments = Vec::with_capacity(assignments.len());
            for a in assignments {
                if let None = existing_lessons.finished_reviews.iter().find(|r| r.assignment_id == a.id) {
                    use_assignments.push(a);
                }
            }
            let mut assignments = use_assignments;

            let subjects_by_id = get_subjects_for_assignments(&assignments, &c).await;
            if let Err(e) = subjects_by_id {
                println!("Error loading subjects: {}", e);
                return;
            }
            let subjects_by_id = subjects_by_id.unwrap();

            let audio_cache = get_audio_path(&p_config);
            if let Err(e) = audio_cache {
                println!("{}", e);
                return;
            }

            let image_cache = get_image_cache(&p_config);
            if let Err(e) = image_cache {
                println!("{}", e);
                return;
            }

            let mut missing_subjs = false; 
            for ass in &assignments {
                if !subjects_by_id.contains_key(&ass.data.subject_id) {
                    missing_subjs = true;
                    break;
                }
            }
            if missing_subjs {
                println!("Some subject data is missing. You may need to run 'wani sync'");
                assignments = assignments
                    .into_iter()
                    .filter(|a| subjects_by_id.contains_key(&a.data.subject_id))
                    .collect_vec();
            }
            if is_user_restricted {
                assignments = assignments
                    .into_iter()
                    .filter(|a| {
                        match subjects_by_id.get(&a.data.subject_id) {
                            None => false,
                            Some(subj) => match subj {
                                Subject::Radical(r) => r.data.level < 4,
                                Subject::Kanji(k) => k.data.level < 4,
                                Subject::Vocab(v) => v.data.level < 4,
                                Subject::KanaVocab(kv) => kv.data.level < 4,
                            }
                        }}).collect_vec();
            }

            let res = do_lessons(assignments, subjects_by_id, audio_cache.unwrap(), &web_config, &p_config, &image_cache.unwrap(), &c, &rate_limit).await;
            match res {
                Ok(_) => {},
                Err(e) => {println!("{:?}", e)},
            }
        },
    }
}

async fn do_lessons(mut assignments: Vec<Assignment>, subjects_by_id: HashMap<i32, Subject>, audio_cache: PathBuf, web_config: &WaniWebConfig, p_config: &ProgramConfig, image_cache: &PathBuf, c: &AsyncConnection, rate_limit: &RateLimitBox) -> Result<(), WaniError> {
    assignments.reverse();
    let batch_size = min(5, assignments.len());
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

    let mut subject_counts = SubjectCounts::default();
    for ass in &assignments {
        match subjects_by_id.get(&ass.data.subject_id).unwrap() {
            Subject::Radical(_) => subject_counts.radical_count += 1,
            Subject::Kanji(_) => subject_counts.kanji_count += 1,
            Subject::Vocab(_) => subject_counts.vocab_count += 1,
            Subject::KanaVocab(_) => subject_counts.vocab_count += 1,
        }
    }

    let mut rev_type = ReviewType::Lesson(subject_counts);
    while assignments.len() > 0 {
        let mut batch = Vec::with_capacity(batch_size);
        //assignments.shuffle(&mut thread_rng());
        for i in (assignments.len() - batch_size..assignments.len()).rev() {
            batch.push(assignments.remove(i));
        }

        let _ = do_lesson_batch(batch, &mut rev_type, &subjects_by_id, image_cache, web_config, c, &audio_tx, p_config, rate_limit).await;
    }

    audio_task.abort();
    Ok(())
}

fn show_lesson_help(term: &Term, align: console::Alignment) {
    let width = term.size().1.into();
    let _ = term.clear_screen();
    let _ = term.write_line(pad_str("Hotkeys", width, align, None).deref());
    let _ = term.write_line(pad_str("?: Show hotkeys menu", width, align, None).deref());
    let _ = term.write_line(pad_str("'n' and 'N' toggle through flashcard pages", width, align, None).deref());
    let _ = term.write_line(pad_str("'a' and 'd' also toggle through flashcard pages", width, align, None).deref());
    let _ = term.write_line(pad_str("arrow keys also toggle through flashcard pages", width, align, None).deref());
    let _ = term.write_line(pad_str("j: play subject audio", width, align, None).deref());
    let _ = term.write_line(pad_str("g: skip to next subject flashcard", width, align, None).deref());
    let _ = term.write_line(pad_str("q: skip to quiz", width, align, None).deref());
    let _ = term.flush();
    let _ = term.read_key();
}

fn show_review_help(term: &Term, align: console::Alignment) {
    let width = term.size().1.into();
    let _ = term.clear_screen();
    let _ = term.write_line(pad_str("Hotkeys", width, align, None).deref());
    let _ = term.write_line(pad_str("?: Show hotkeys menu", width, align, None).deref());
    let _ = term.write_line(pad_str("j: play subject audio", width, align, None).deref());
    let _ = term.write_line(pad_str("f: open/close subject information", width, align, None).deref());
    let _ = term.write_line(pad_str("'n' and 'N' toggle through information pages", width, align, None).deref());
    let _ = term.flush();
    let _ = term.read_key();
}

async fn do_lesson_batch(mut batch: Vec<Assignment>, subj_counts: &mut ReviewType, subjects: &HashMap<i32, Subject>, image_cache: &PathBuf, web_config: &WaniWebConfig, conn: &AsyncConnection, audio_tx: &Sender<AudioMessage>, p_config: &ProgramConfig, rate_limit: &RateLimitBox) -> Result<(), WaniError> {
    if batch.len() == 0 {
        return Ok(());
    }

    let term = Term::buffered_stdout();
    let align = console::Alignment::Center;
    let wfmt_args = get_wfmt_args(&term);

    let mut index = 0;
    'flashcards: loop {
        if index >= batch.len() {
            break 'flashcards;
        }

        let assignment = &batch[index];
        let subject = subjects.get(&assignment.data.subject_id).unwrap();
        let characters = get_chars_for_subj(&subject, image_cache, 100, web_config).await;
        if let Err(_) = characters {
            index += 1;
            continue 'flashcards;
        }

        let primary_meaning = match subject {
            Subject::Radical(r) => r.primary_meanings().next(),
            Subject::Kanji(k) => k.primary_meanings().next(),
            Subject::Vocab(v) => v.primary_meanings().next(),
            Subject::KanaVocab(kv) => kv.primary_meanings().next(),
        };
        let meaning_line = if let Some(meaning) = primary_meaning {
            let padded_meaning = pad_str(meaning, term.size().1.into(), align, None);
            Some(match subject {
                Subject::Radical(_) => style(padded_meaning).white().on_blue().to_string(),
                Subject::Kanji(_) => style(padded_meaning).white().on_red().to_string(),
                _ => style(padded_meaning).white().on_magenta().to_string(),
            })
        } else { None };

        let mut card_page = 0;
        'card: loop {
            let (width, text_width, _) = print_lesson_screen(&term, &meaning_line, subj_counts, &subject, image_cache, web_config).await?;
            let lines = get_lesson_info_lines(subject, card_page, &wfmt_args, text_width, conn, align, width).await;
            if let None = lines {
                index += 1;
                break 'card;
            }

            for line in &lines.unwrap() {
                term.write_line(&pad_str(line, width, align, None))?;
            }
            term.flush()?;

            match term.read_key()? {
                console::Key::ArrowLeft => {
                    if card_page > 0 {
                        card_page -= 1;
                    }
                },
                console::Key::ArrowRight => {
                    card_page = card_page.wrapping_add(1);
                },
                console::Key::Char(c) => {
                    match c {
                        '?' => show_lesson_help(&term, align),
                        'q' | 'Q' => break 'flashcards,
                        'g' | 'G' => { 
                            index += 1;
                            break 'card;
                        },
                        'n' | 'd' | 'D' => {
                            card_page = card_page.wrapping_add(1);
                        },
                        'N' | 'a' | 'A' => {
                            if card_page > 0 {
                                card_page -= 1;
                            }
                        },
                        'j' | 'J' => {
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
                                    audios: audios.iter()
                                        .map(|a| AudioInfo {
                                            url: a.url.clone(),
                                            content_type: a.content_type.clone(),
                                        }).collect_vec(),

                                }).await;
                            }
                        },
                        _ => {},
                    }
                },
                _ => {},
            }
        }
    }

    let now = Utc::now();
    let mut reviews = HashMap::with_capacity(batch.len());
    for a in &batch {
        reviews.insert(a.id, wanidata::NewReview {
            id: None,
            assignment_id: a.id,
            created_at: now,
            incorrect_meaning_answers: 0,
            incorrect_reading_answers: 0,
            status: wanidata::ReviewStatus::NotStarted,
            available_at: None, // Lesson reviews should not be available
        });
    }

    do_reviews_inner(subjects, web_config, p_config, image_cache, &mut reviews, &mut batch, subj_counts, audio_tx, conn).await?;

    let _ = save_lessons(reviews, rate_limit, web_config, conn).await;

    Ok(())
}

async fn do_reviews_inner<'a>(subjects: &HashMap<i32, Subject>, web_config: &WaniWebConfig, p_config: &ProgramConfig, image_cache: &PathBuf, reviews: &mut HashMap<i32, NewReview>, batch: &mut Vec<Assignment>, rev_type: &mut ReviewType, audio_tx: &Sender<AudioMessage>, connection: &AsyncConnection) -> Result<(), WaniError> {
    let term = Term::buffered_stdout();
    let rng = &mut thread_rng();
    let align = console::Alignment::Center;
    let correct_msg = if p_config.colorblind { Some("Correct") } else { None };
    let incorrect_msg = if p_config.colorblind { Some("Inorrect") } else { None };
    let wfmt_args = get_wfmt_args(&term);
    let mut input = String::new();
    'subject: loop {
        if batch.is_empty() {
            break 'subject;
        }
        batch.shuffle(rng);
        /*  
            let assignment = batch.iter().find_or_last(|a| { 
            let subj = subjects.get(&a.data.subject_id).unwrap();
            if let wanidata::Subject::KanaVocab(_) = subj {
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
        let characters = get_chars_for_subj(subject, image_cache, 100, web_config).await;
        if let Err(_) = characters {
            batch.pop();
            continue 'subject;
        }

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

        let mut toast = None;

        'input: loop {
            input.clear();
            let (width, _, char_lines) = print_review_screen(&term, rev_type, align, subject, review_type_text, &toast, image_cache, web_config, "", None).await?;
            term.move_cursor_to(width / 2, 2 + char_lines.len())?;
            term.flush()?;

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
                        if input.len() > 0 {
                            input.push(c);
                        }
                        else {
                            match c {
                                '?' => show_review_help(&term, align),
                                _ => input.push(c),
                            }
                        }
                    },
                    _ => {},
                };

                kana_input = input.to_kana_with_opt(wana_kana::Options { 
                    imemode: true,
                    ..Default::default()
                });
                vis_input = if is_meaning { &input } else { &kana_input };
                let (width, _, char_lines) = print_review_screen(&term, rev_type, align, subject, review_type_text, &toast, image_cache, web_config, &vis_input, None).await?;
                let input_width = console::measure_text_width(&vis_input);
                term.move_cursor_to(width / 2 + vis_input.chars().count() / 2, 2 + char_lines.len())?;
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
                            match rev_type {
                                ReviewType::Review(stats) => {
                                    stats.done += 1;
                                },
                                ReviewType::Lesson(subj_counts) => {
                                    match subject {
                                        Subject::Radical(_) => subj_counts.radical_count -= 1,
                                        Subject::Kanji(_) => subj_counts.kanji_count -= 1,
                                        _ => subj_counts.vocab_count -= 1,
                                    }
                                },
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
                                    match rev_type {
                                        ReviewType::Review(stats) => {
                                            stats.done += 1;
                                        },
                                        ReviewType::Lesson(subj_counts) => {
                                            match subject {
                                                Subject::Radical(_) => subj_counts.radical_count -= 1,
                                                Subject::Kanji(_) => subj_counts.kanji_count -= 1,
                                                _ => subj_counts.vocab_count -= 1,
                                            }
                                        },
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
                    if let ReviewType::Review(stats) = rev_type {
                        stats.failed += 1;
                    }
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
                if let ReviewType::Review(stats) = rev_type {
                    stats.guesses += 1;
                }
            }

            let (width, _, char_lines) = print_review_screen(&term, rev_type, align, subject, review_type_text, &toast, image_cache, web_config, &vis_input, Some(&tuple.2)).await?;
            let input_width = console::measure_text_width(&vis_input);
            term.move_cursor_to(width / 2 + vis_input.chars().count() / 2, 2 + char_lines.len())?;
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
                            '?' => if !tuple.0 {
                                show_review_help(&term, align)
                            },
                            'f' | 'F' => {
                                if !tuple.0 { // Don't show info if the user isn't finished
                                              // guessing
                                    info_status = match info_status {
                                        InfoStatus::Hidden => InfoStatus::Open(0),
                                        InfoStatus::Open(_) => InfoStatus::Hidden,
                                    };
                                }
                            },
                            'n' => {
                                if !tuple.0 { // Don't show info if the user isn't finished
                                              // guessing
                                    info_status = match info_status {
                                        InfoStatus::Hidden => InfoStatus::Open(0),
                                        InfoStatus::Open(n) => InfoStatus::Open(n.wrapping_add(1)),
                                    };
                                }
                            },
                            'N' => {
                                if !tuple.0 { // Don't show info if the user isn't finished
                                              // guessing
                                    info_status = match info_status {
                                        InfoStatus::Hidden => InfoStatus::Open(0),
                                        InfoStatus::Open(n) => InfoStatus::Open(n.wrapping_sub(1)),
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
                                            audios: audios.iter().map(|a| AudioInfo {
                                                url: a.url.clone(),
                                                content_type: a.content_type.clone(),
                                            }).collect_vec(),
                                        }).await;
                                    }
                                }
                            },
                            _ => {},
                        }
                    },
                    _ => {},
                }

                let (width, text_width, char_line) = print_review_screen(&term, rev_type, align, subject, review_type_text, &toast, image_cache, web_config, &vis_input, Some(&tuple.2)).await?;
                if let InfoStatus::Open(info_status) = info_status {
                    let lines = get_info_lines(&subject, info_status, &wfmt_args, is_meaning, connection, text_width, width).await;
                    for line in &lines {
                        term.write_line(&pad_str(line, width, align, None))?;
                    }

                }

                let input_width = console::measure_text_width(&vis_input);
                term.move_cursor_to(width / 2 + vis_input.chars().count() / 2, 2 + char_lines.len())?;
                term.flush()?;
            }

            if !tuple.0 {
                break 'input;
            }

            toast = None;
            let (width, _, char_line) = print_review_screen(&term, rev_type, align, subject, review_type_text, &toast, image_cache, web_config, &"", None).await?;
            term.move_cursor_to(width / 2, 2 + char_line.len())?;
            term.flush()?;
        }
    }

    Ok(())
}

fn get_wfmt_args(term: &Term) -> WaniFmtArgs {
    let blue_tag = format!("\x1b[{}m", 4 + 40);
    let red_tag = format!("\x1b[{}m", 1 + 40);
    let magenta_tag = format!("\x1b[{}m", 5 + 40);
    let cyan_tag = format!("\x1b[{}m", 6 + 40);
    let green_tag = format!("\x1b[{}m", 2 + 40);
    //let gray_tag = format!("\x1b[48;5;{}m", 145);
    if term.features().colors_supported() {
        wanidata::WaniFmtArgs {
            radical_args: wanidata::WaniTagArgs {
                open_tag: blue_tag,
                close_tag: "\x1b[0m".into(),
            },
            kanji_args: wanidata::WaniTagArgs {
                open_tag: red_tag,
                close_tag: "\x1b[0m".into(),
            },
            vocab_args: wanidata::WaniTagArgs {
                open_tag: magenta_tag,
                close_tag: "\x1b[0m".into(),
            },
            meaning_args: wanidata::WaniTagArgs {
                open_tag: cyan_tag.clone(),
                close_tag: "\x1b[0m".into(),
            },
            reading_args: wanidata::WaniTagArgs {
                open_tag: cyan_tag,
                close_tag: "\x1b[0m".into(),
            },
            ja_args: wanidata::WaniTagArgs {
                open_tag: green_tag,
                close_tag: "\x1b[0m".into(),
            },
        }
    }
    else {
        WaniFmtArgs::default()
    }
}

async fn command_review(args: &Args) {
    async fn do_reviews(assignments: &mut Vec<Assignment>, subjects: HashMap<i32, Subject>, audio_cache: PathBuf, web_config: &WaniWebConfig, p_config: &ProgramConfig, image_cache: &PathBuf, conn: &AsyncConnection, rate_limit: &RateLimitBox, first_batch: Option<Vec<(Assignment, NewReview)>>) -> Result<(), WaniError> {
        assignments.reverse();
        let total_assignments = assignments.len() + if let Some(batch) = &first_batch { batch.len() } else { 0 };
        let mut first_batch = first_batch;
        let ideal_batch_size = 20;
        let mut batch_size;
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
        let mut first_reviews = None;
        let stats = ReviewStats {
            total_reviews: total_assignments,
            ..Default::default()
        };
        let mut stats = ReviewType::Review(stats);
        loop {
            if let None = first_batch {
                if assignments.len() == 0 {
                    break;
                }
            }

            let mut batch = match first_batch { 
                None => { 
                    batch_size = min(ideal_batch_size, assignments.len());
                    let mut b = Vec::with_capacity(batch_size);
                    assignments.shuffle(&mut thread_rng());
                    for i in (assignments.len() - batch_size..assignments.len()).rev() {
                        b.push(assignments.remove(i));
                    }
                    b
                },
                Some(b) => {
                    println!("Resuming saved batch of reviews");
                    let mut batch = Vec::with_capacity(b.len());
                    let mut revs = HashMap::with_capacity(b.len());
                    for (assignment, review) in b {
                        batch.push(assignment);
                        revs.insert(review.assignment_id, review);
                    }
                    first_batch = None;
                    first_reviews = Some(revs);
                    batch
                }
            };

            let mut reviews = if let Some(r) = first_reviews { 
                first_reviews = None;
                r
            } else {
                let mut reviews = HashMap::with_capacity(batch.len());
                let now = Utc::now();
                for nr in batch.iter().map(|a| wanidata::NewReview {
                    id: None,
                    assignment_id: a.id,
                    created_at: now,
                    incorrect_meaning_answers: 0,
                    incorrect_reading_answers: 0,
                    status: wanidata::ReviewStatus::NotStarted,
                    available_at: a.data.available_at,
                }) {
                    reviews.insert(nr.assignment_id, nr);
                }
                reviews
            };

            let res = do_reviews_inner(&subjects, web_config, p_config, image_cache, &mut reviews, &mut batch, &mut stats, &audio_tx, conn).await;
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

        audio_task.abort();
        review_result.unwrap_or(Ok(()))
    }

    let p_config = get_program_config(args);
    if let Err(e) = &p_config {
        println!("{}", e);
    }
    let p_config = p_config.unwrap();

    let rate_limit = Arc::new(Mutex::new(None));
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();

    let conn = setup_async_connection(&p_config).await;
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            let mut ass_cache_info = CacheInfo { id: CACHE_TYPE_SUBJECTS, ..Default::default() };
            let mut c_infos = get_all_cache_infos(&c, false).await;
            if let Ok(c_infos) = &mut c_infos {
                if let Some(info) = c_infos.remove(&CACHE_TYPE_SUBJECTS) {
                    ass_cache_info = info;
                }
            }

            println!("Syncing assignments. . .");
            let is_user_restricted = is_user_restricted(&web_config, &c, &rate_limit).await;
            let _ = sync_assignments(&c, &web_config, ass_cache_info, &rate_limit, is_user_restricted).await;

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

            let existing_reviews = load_existing_reviews(&c, &assignments).await;
            let existing_reviews = match existing_reviews {
                Ok(existing_reviews) => { 
                    existing_reviews 
                },
                Err(e) => {
                    println!("Error loading existing reviews: {}", e);
                    LoadedReviews::default()
                },
            };

            for review in existing_reviews.invalid_reviews {
                let _ = c.call(move |conn| {
                    conn.execute(wanisql::REMOVE_REVIEW, params![review.assignment_id])?;
                    Ok(())
                }).await;
            }

            let _ = save_reviews_to_wanikani(existing_reviews.finished_reviews.iter(), &rate_limit, &web_config, &c).await;
            for review in existing_reviews.finished_reviews.iter() {
                if let Some(t) = assignments.iter().find_position(|a| a.id == review.assignment_id) {
                    assignments.remove(t.0);
                }
            }

            let subjects_by_id = get_subjects_for_assignments(&assignments, &c).await;
            if let Err(e) = subjects_by_id {
                println!("Error loading subjects: {}", e);
                return;
            }
            let subjects_by_id = subjects_by_id.unwrap();

            let first_batch = if existing_reviews.in_progress_reviews.len() == 0 { None } else {
                let mut first_batch = Vec::with_capacity(existing_reviews.in_progress_reviews.len());
                for rev in existing_reviews.in_progress_reviews {
                    if let Some((index, _)) = assignments.iter().find_position(|a| a.id == rev.assignment_id) {
                        first_batch.push((assignments.remove(index), rev));
                    }
                }
                Some(first_batch)
            };

            let audio_cache = get_audio_path(&p_config);
            if let Err(e) = audio_cache {
                println!("{}", e);
                return;
            }

            let image_cache = get_image_cache(&p_config);
            if let Err(e) = image_cache {
                println!("{}", e);
                return;
            }

            let _ = ctrlc::set_handler(move || {
                println!("\nreceived Ctrl+C!\nSaving reviews...");
            });

            let mut missing_subjs = false; 
            for ass in &assignments {
                if !subjects_by_id.contains_key(&ass.data.subject_id) {
                    missing_subjs = true;
                    break;
                }
            }
            if missing_subjs {
                println!("Some subject data is missing. You may need to run 'wani sync'");
                assignments = assignments
                    .into_iter()
                    .filter(|a| subjects_by_id.contains_key(&a.data.subject_id))
                    .collect_vec();
            }
            if is_user_restricted {
                assignments = assignments
                    .into_iter()
                    .filter(|a| {
                        match subjects_by_id.get(&a.data.subject_id) {
                            None => false,
                            Some(subj) => match subj {
                                Subject::Radical(r) => r.data.level < 4,
                                Subject::Kanji(k) => k.data.level < 4,
                                Subject::Vocab(v) => v.data.level < 4,
                                Subject::KanaVocab(kv) => kv.data.level < 4,
                            }
                        }}).collect_vec();
            }

            let res = do_reviews(&mut assignments, subjects_by_id, audio_cache.unwrap(), &web_config, &p_config, &image_cache.unwrap(), &c, &rate_limit, first_batch).await;
            match res {
                Ok(_) => {},
                Err(e) => {println!("{:?}", e)},
            }
        },
    };
}

async fn get_subjects_for_assignments(assignments: &[Assignment], c: &AsyncConnection) -> Result<HashMap<i32, Subject>, WaniError> {
    let mut subjects_by_id = HashMap::new();
    let mut r_ids = vec![];
    let mut k_ids = vec![];
    let mut v_ids = vec![];
    let mut kv_ids = vec![];
    for ass in assignments {
        match ass.data.subject_type {
            SubjectType::Radical => r_ids.push(ass.data.subject_id),
            SubjectType::Kanji => k_ids.push(ass.data.subject_id),
            SubjectType::Vocab => v_ids.push(ass.data.subject_id),
            SubjectType::KanaVocab => kv_ids.push(ass.data.subject_id),
        }
    }

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
    }).await?;
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
    }).await?;
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
    }).await?;
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
    }).await?;
    for s in kana_vocab {
        subjects_by_id.insert(s.id, wanidata::Subject::KanaVocab(s));
    }

    Ok(subjects_by_id)
}

async fn list_vocab_from_ids(conn: &AsyncConnection, ids: Vec<i32>, label: &str) -> Vec<String> {
    let mut lines = vec![];
    match lookup_vocab(conn, ids).await {
        Ok(vocab) => {
            let mut i = 0;
            let vocab_in_line = 6;
            lines.push(label.to_owned());
            while i < vocab.len() {
                let mut j = 0;
                let mut vocab_line = vec![];
                while i < vocab.len() && j < vocab_in_line {
                    vocab_line.push(&vocab[i].data.characters);
                    i += 1;
                    j += 1;
                }
                lines.push(vocab_line.iter().join(", "));
            }
        },
        Err(e) => { 
            lines.push(format!("Error looking up vocab. {}", e));
        }
    }
    lines
}

async fn list_radicals_from_ids(conn: &AsyncConnection, ids: Vec<i32>, label: &str) -> Vec<String> {
    let mut lines = vec![];
    match lookup_radical(conn, ids).await {
        Ok(radicals) => {
            let mut i = 0;
            let radicals_in_line = 3;
            lines.push(label.to_owned());
            while i < radicals.len() {
                let mut j = 0;
                let mut radical_line = vec![];
                while i < radicals.len() && j < radicals_in_line {
                    if let Some(characters) = &radicals[i].data.characters {
                        radical_line.push(format!("{} {}", characters, &radicals[i].primary_meanings().next().unwrap_or(&String::from(""))));
                        j += 1;
                    }
                    i += 1;
                }
                lines.push(radical_line.iter().join(", "));
            }
        },
        Err(e) => { 
            lines.push(format!("Error looking up radicals. {}", e));
        }
    }
    lines

}

async fn list_kanji_from_ids(conn: &AsyncConnection, ids: Vec<i32>, label: &str) -> Vec<String> {
    let mut lines = vec![];
    match lookup_kanji(conn, ids).await {
        Ok(kanji) => {
            let mut i = 0;
            let kanji_in_line = 6;
            lines.push(label.to_owned());
            while i < kanji.len() {
                let mut j = 0;
                let mut kanji_line = vec![];
                while i < kanji.len() && j < kanji_in_line {
                    kanji_line.push(&kanji[i].data.characters);
                    i += 1;
                    j += 1;
                }
                lines.push(kanji_line.iter().join(", "));
            }
        },
        Err(e) => { 
            lines.push(format!("Error looking up kanji. {}", e));
        }
    }
    lines
}

fn get_context_sentences(sentences: &Vec<ContextSentence>, text_width: usize, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let left = console::Alignment::Left;
    lines.push("Context Sentences:".to_owned());
    for sent in sentences {
        //lines.push(pad_str("English:", width, left, None).to_string());
        let mut sent_lines = vec![];
        split_str_by_len(&sent.ja, text_width, &mut sent_lines);
        for ele in &sent_lines {
            let mut line = String::from("\t");
            line.push_str(&pad_str(&ele, width, left, None).to_string());
            lines.push(line);
        }
        sent_lines.clear();
        //lines.push(pad_str("日本語:", width, left, None).to_string());
        split_str_by_len(&sent.en, text_width, &mut sent_lines);
        for ele in sent_lines {
            let mut line = String::from("\t");
            line.push_str(&pad_str(&ele, width, left, None).to_string());
            lines.push(line);
        }
        lines.push("".into());
    }
    lines
}

async fn get_lesson_info_lines(subject: &Subject, card_page: usize, wfmt_args: &WaniFmtArgs, text_width: usize, conn: &AsyncConnection, align: console::Alignment, width: usize) -> Option<Vec<String>> { 
    match subject {
        Subject::Radical(r) => {
            let num_pages = 2;
            if card_page >= num_pages {
                return None;
            }
            let card_page = card_page % num_pages;
            Some(match card_page {
                0 => {
                    let mut lines = vec![];
                    let mnemonic = wanidata::format_wani_text(&r.data.meaning_mnemonic, wfmt_args);
                    split_str_by_len(&mnemonic, text_width, &mut lines);
                    lines
                },
                1 => {
                    let label = "Kanji Examples:";
                    list_kanji_from_ids(conn, r.data.amalgamation_subject_ids.clone(), label).await
                },
                _ => { vec![] },
            })
        },
        Subject::Kanji(k) => {
            let num_pages = 4;
            if card_page >= num_pages {
                return None;
            }
            Some(match card_page {
                0 => {
                    let label = "Radical Composition:";
                    list_radicals_from_ids(conn, k.data.component_subject_ids.clone(), label).await
                },
                1 => {
                    kanji_meaning_lines(k, text_width, wfmt_args)
                },
                2 => {
                    // TODO - list on'yomi vs kunyomi etc
                    kanji_reading_lines(k, text_width, wfmt_args)
                },
                3 => {
                    let label = "Vocabulary Examples:";
                    list_vocab_from_ids(conn, k.data.amalgamation_subject_ids.clone(), label).await
                },
                _ => { vec![] },
            })
        },
        Subject::Vocab(v) => { 
            let num_pages = 4;
            if card_page >= num_pages {
                return None;
            }
            Some(match card_page {
                0 => {
                    vocab_kanji_composition(v, conn, "Kanji Composition:").await
                },
                1 => {
                    vocab_meaning_lines(v, text_width, wfmt_args)
                },
                2 => {
                    vocab_reading_lines(v, text_width, wfmt_args)
                },
                3 => {
                    get_context_sentences(&v.data.context_sentences, text_width, width)
                },
                _ => { vec![] },
            })
        },
        Subject::KanaVocab(kv) => {
            let num_pages = 2;
            if card_page >= num_pages {
                return None;
            }
            Some(match card_page {
                0 => {
                    kana_vocab_meaning_lines(kv, text_width, wfmt_args)
                },
                1 => {
                    get_context_sentences(&kv.data.context_sentences, text_width, width)
                },
                _ => { vec![] },
            })
        }
    }
}

async fn get_info_lines(subject: &Subject, info_status: usize, wfmt_args: &WaniFmtArgs, is_meaning: bool, conn: &AsyncConnection, text_width: usize, width: usize) -> Vec<String> {
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
                        lines.push(meanings);
                    }
                    else {
                        lines.push("Radical name not found".to_owned())
                    }
                    let mnemonic = wanidata::format_wani_text(&r.data.meaning_mnemonic, wfmt_args);
                    lines.push("---".to_owned());
                    split_str_by_len(&mnemonic, text_width, &mut lines);
                    lines
                },
                1 => {
                    let label = "Found in Kanji:";
                    list_kanji_from_ids(conn, r.data.amalgamation_subject_ids.clone(), label).await
                },
                _ => { vec![] }
            }
        },

        // 0 - kanji meaning, mnemonic, TODO hint, meaning/reading hint, user note
        // TODO 1 - user synonym, user note
        // 1 - kanji reading, mnemonic, TODO hint
        // 2 - visually similar kanji
        // 3 - found in vocab
        // 4 - radical combination
        Subject::Kanji(k) => {
            let num_choices = 5;
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
                    kanji_meaning_lines(k, text_width, wfmt_args)
                },
                1 => {
                    kanji_reading_lines(k, text_width, wfmt_args)
                },
                2 => {
                    let label = "Visually Similar Kanji:";
                    list_kanji_from_ids(conn, k.data.visually_similar_subject_ids.clone(), label).await
                },
                3 => {
                    let label = "Found in Vocab:";
                    list_vocab_from_ids(conn, k.data.amalgamation_subject_ids.clone(), label).await
                },
                4 => {
                    let label = "Radical Combination:";
                    list_radicals_from_ids(conn, k.data.component_subject_ids.clone(), label).await
                },
                _ => { vec![] }
            }
        },

        // 0 - vocab meaning, mnemonic, TODO hint, part of speech
        // TODO 1 - user synonym, user note
        // 1 - vocab reading, mnemonic, TODO hint
        // TODO 2 - Context Pt 1:
        //      - patterns for use
        //      - common word combinations
        // 2 - Context Pt 2:
        //      - context sentences
        // 3 - Kanji composition
        Subject::Vocab(v) => {
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
                    vocab_meaning_lines(v, text_width, wfmt_args)
                },
                1 => {
                    vocab_reading_lines(v, text_width, wfmt_args)
                },
                2 => {
                    get_context_sentences(&v.data.context_sentences, text_width, width)
                },
                3 => {
                    vocab_kanji_composition(v, conn, "Kanji Composition:").await
                },
                _ => { vec![] },
            }
        },

        // 0 - kana vocab meaning, mnemonic, TODO hint, part of speech
        // TODO 1 - user synonym, user note
        // 1 - context sentences
        Subject::KanaVocab(kv) => {
            let num_choices = 2;
            let info_status = info_status % num_choices;

            match info_status {
                0 => {
                    kana_vocab_meaning_lines(kv, text_width, wfmt_args)
                },
                1 => {
                    get_context_sentences(&kv.data.context_sentences, text_width, width)
                },
                _ => { vec![] },
            }

        },
    }
}

fn kana_vocab_meaning_lines(kv: &wanidata::KanaVocab, text_width: usize, wfmt_args: &WaniFmtArgs) -> Vec<String> {
    let mut lines = vec![];
    let meanings = kv.primary_meanings()
        .join(", ");
    if meanings.len() > 0 {
        lines.push(meanings);
    }
    let alt_meanings = kv.alt_meanings()
        .join(", ");
    if alt_meanings.len() > 0 {
        lines.push(alt_meanings);
    }
    lines.push("---".to_owned());
    let mnemonic = wanidata::format_wani_text(&kv.data.meaning_mnemonic, &wfmt_args);
    split_str_by_len(&mnemonic, text_width, &mut lines);
    lines
}

async fn vocab_kanji_composition(v: &wanidata::Vocab, conn: &AsyncConnection, label: &str) -> Vec<String> {
    let mut lines = vec![];
    lines.push(label.to_owned());
    match lookup_kanji(conn, v.data.component_subject_ids.clone()).await {
        Ok(kanji) => {
            let mut i = 0;
            let kanji_in_line = 3;
            while i < kanji.len() {
                let mut j = 0;
                let mut kanji_line = vec![];
                while i < kanji.len() && j < kanji_in_line {
                    kanji_line.push(format!("{} {}", 
                                            &kanji[i].data.characters, 
                                            &kanji[i].primary_meanings()
                                            .map(|m| m.to_owned())
                                            .next()
                                            .unwrap_or("".to_owned())));
                    i += 1;
                    j += 1;
                }
                lines.push(kanji_line.iter().join(", "));
            }

        },
        Err(e) => {
            lines.push(format!("Error looking up kanji: {}", e));
        }
    };
    lines
}

fn vocab_reading_lines(v: &wanidata::Vocab, text_width: usize, wfmt_args: &WaniFmtArgs) -> Vec<String> {
    let mut lines = vec![];
    let readings = v.primary_readings()
        .join(", ");
    if readings.len() > 0 {
        lines.push(readings);
    }
    let alt_readings = v.alt_readings()
        .join(", ");
    if alt_readings.len() > 0 {
        lines.push(alt_readings);
    }
    lines.push("---".to_owned());
    let mnemonic = wanidata::format_wani_text(&v.data.reading_mnemonic, &wfmt_args);
    split_str_by_len(&mnemonic, text_width, &mut lines);
    lines
}

fn vocab_meaning_lines(v: &wanidata::Vocab, text_width: usize, wfmt_args: &WaniFmtArgs) -> Vec<String> {
    let mut lines = vec![];
    let meanings = v.primary_meanings()
        .join(", ");
    if meanings.len() > 0 {
        lines.push(meanings);
    }
    let alt_meanings = v.alt_meanings()
        .join(", ");
    if alt_meanings.len() > 0 {
        lines.push(alt_meanings);
    }
    if v.data.parts_of_speech.len() > 0 {
        lines.push("---".to_owned());
        lines.push(v.data.parts_of_speech.join(", "));
    }
    lines.push("---".to_owned());
    let mnemonic = wanidata::format_wani_text(&v.data.meaning_mnemonic, &wfmt_args);
    split_str_by_len(&mnemonic, text_width, &mut lines);
    lines
}

fn kanji_reading_lines(k: &wanidata::Kanji, text_width: usize, wfmt_args: &WaniFmtArgs) -> Vec<String> {
    let mut lines = vec![];
    let readings = k.primary_readings()
        .join(", ");
    if readings.len() > 0 {
        lines.push(readings);
    }
    let alt_readings = k.alt_readings()
        .join(", ");
    if alt_readings.len() > 0 {
        lines.push(alt_readings);
    }
    lines.push("---".to_owned());
    let mnemonic = wanidata::format_wani_text(&k.data.reading_mnemonic, &wfmt_args);
    split_str_by_len(&mnemonic, text_width, &mut lines);
    lines
}

fn kanji_meaning_lines(k: &wanidata::Kanji, text_width: usize, wfmt_args: &WaniFmtArgs) -> Vec<String> {
    let mut lines = vec![];
    let meanings = k.primary_meanings()
        .join(", ");
    if meanings.len() > 0 {
        lines.push(meanings);
    }
    let alt_meanings = k.alt_meanings()
        .join(", ");
    if alt_meanings.len() > 0 {
        lines.push(alt_meanings);
    }
    lines.push("---".to_owned());
    let mnemonic = wanidata::format_wani_text(&k.data.meaning_mnemonic, wfmt_args);
    split_str_by_len(&mnemonic, text_width, &mut lines);
    lines
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

async fn lookup_radical(conn: &AsyncConnection, ids: Vec<i32>) -> Result<Vec<wanidata::Radical>, WaniError> {
    Ok(conn.call(move |c| { 
        let stmt = c.prepare(&wanisql::select_radicals_by_id(ids.len()));
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| wanisql::parse_radical(r)
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

async fn play_audio_for_subj(id: i32, audios: Vec<AudioInfo>, audio_cache: &PathBuf, web_config: &WaniWebConfig) -> Result<(), WaniError> {
    fn get_audio_path(audio: &AudioInfo, audio_cache: &PathBuf, id: i32, index: usize) -> Option<PathBuf> {
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

fn split_str_by_len(s: &str, l: usize, v: &mut Vec<String>) {
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

async fn load_existing_lessons(c: &AsyncConnection, assignments: &Vec<wanidata::Assignment>) -> Result<LoadedReviews, tokio_rusqlite::Error> {
    let assignments: HashSet<i32, RandomState> = HashSet::from_iter(assignments.iter().map(|a| a.id));

    return c.call(move |c| { 
        let stmt = c.prepare(wanisql::SELECT_LESSONS);
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map([], |r| parse_review(r)
                                     .or_else
                                     (|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                    Ok(reviews) => {
                        let mut loaded = LoadedReviews::default(); 
                        for r in reviews {
                            if let Err(r) = r {
                                println!("Error loading review: {}", r);
                                continue;
                            }
                            let r = r.unwrap();

                            if let ReviewStatus::Done = r.status {
                                if assignments.contains(&r.assignment_id) {
                                    loaded.finished_reviews.push(r);
                                }
                                else {
                                    loaded.invalid_reviews.push(r);
                                }
                            }
                            else {
                                loaded.invalid_reviews.push(r);
                            }
                        }

                        Ok(loaded)
                    },
                    Err(e) => {Err(tokio_rusqlite::Error::Rusqlite(e))},
                }
            }
        }
    }).await;
}

async fn load_existing_reviews(c: &AsyncConnection, assignments: &Vec<wanidata::Assignment>) -> Result<LoadedReviews, tokio_rusqlite::Error> {
    let mut available_at_by_id = HashMap::with_capacity(assignments.len());
    for ass in assignments {
        if let Some(available_at) = ass.data.available_at {
            available_at_by_id.insert(ass.id, available_at);
        }
    }

    return c.call(move |c| { 
        let stmt = c.prepare(wanisql::SELECT_REVIEWS);
        match stmt {
            Err(e) => {
                return Err(tokio_rusqlite::Error::Rusqlite(e));
            },
            Ok(mut stmt) => {
                match stmt.query_map([], |r| parse_review(r)
                                     .or_else
                                     (|e| Err(rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Null, Box::new(e))))) {
                    Ok(reviews) => {
                        let mut loaded_revs = LoadedReviews {
                            invalid_reviews: vec![],
                            finished_reviews: vec![],
                            in_progress_reviews: vec![]
                         };

                        for r in reviews {
                            if let Err(r) = r {
                                println!("Error loading review: {}", r);
                                continue;
                            }
                            let r = r.unwrap();
                            if let Some(available_at) = r.available_at {
                                if let Some(ass_available) = available_at_by_id.get(&r.assignment_id) {
                                    if ass_available != &available_at {
                                        loaded_revs.invalid_reviews.push(r);
                                        continue;
                                    }
                                }
                                else {
                                    // If there is no assignment for a review, invalidate it.
                                    loaded_revs.invalid_reviews.push(r);
                                    continue;
                                }
                            }

                            match r.status {
                                ReviewStatus::Done => {
                                    loaded_revs.finished_reviews.push(r);
                                },
                                _ => loaded_revs.in_progress_reviews.push(r),
                            }
                        }

                        Ok(loaded_revs)
                    },
                    Err(e) => {Err(tokio_rusqlite::Error::Rusqlite(e))},
                }
            }
        }
    }).await;
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

    let conn = setup_async_connection(&p_config).await;
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            sync_all(&web_config, &c, ignore_cache).await;
        },
    };
}

async fn sync_assignments(conn: &AsyncConnection, web_config: &WaniWebConfig, cache_info: CacheInfo, rate_limit: &RateLimitBox, is_user_restricted: bool) -> Result<SyncResult, WaniError> {
    let mut next_url = Some("https://api.wanikani.com/v2/assignments".to_owned());

    let mut assignments = vec![];
    let mut last_request_time: Option<DateTime<Utc>> = None;
    let mut headers = None;
    while let Some(url) = next_url {
        next_url = None;
        let mut query: Vec<(&str, &str)> = vec![];
        if let Some(after) = &cache_info.updated_after {
            query.push(("updated_after", after));
        }
        if is_user_restricted {
            query.push(("levels", "1,2,3"));
        }
        
        let info = RequestInfo::<()> {
            url,
            method: RequestMethod::Get,
            query: if query.len() > 0 { Some(query) } else { None }, 
            headers: if let Some(tag) = &cache_info.last_modified {
                Some(vec![(reqwest::header::LAST_MODIFIED.to_string(), tag.to_owned())])
            } else { None },
            ..Default::default()
        };

        last_request_time = Some(Utc::now());
        match send_throttled_request(info, rate_limit.clone(), web_config.clone()).await {
            Ok(t) => {
                headers = Some(t.1);
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
        let mut last_modified = None;
        if let Some(h) = headers {
            if let Some(tag) = h.get(reqwest::header::LAST_MODIFIED) {
                if let Ok(t) = tag.to_str() {
                    last_modified = Some(t.to_owned());
                }
            }
        }

        match update_cache(last_modified, CACHE_TYPE_ASSIGNMENTS, time, None, &conn).await {
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

/// Whether user is restricted to the free-tier WaniKani (levels 1-3).
/// This checks the user's subscription level, caching the result until the subscription ends,
/// otherwise re-checking every week.
async fn is_user_restricted(web_config: &WaniWebConfig, conn: &AsyncConnection, rate_limit: &RateLimitBox) -> bool {
    match get_user_info(web_config, conn, rate_limit).await {
        Ok(user) => {
            user.data.subscription.max_level_granted < 60
        },
        Err(_) => {
            false
        },
    }
}

async fn get_user_info(web_config: &WaniWebConfig, conn: &AsyncConnection, rate_limit: &RateLimitBox) -> Result<wanidata::User, WaniError> {
    let mut cache_info = conn.call(|conn| {
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
    }).await?;
    let user_cache = cache_info.remove(&CACHE_TYPE_USER);

    let users = select_data(wanisql::SELECT_USER, conn, wanisql::parse_user, []).await?;

    let mut load_user = users.len() == 0;
    if !load_user {
        load_user = match &user_cache {
            Some(user_cache) => { 
                match &user_cache.updated_after {
                    Some(updated_after) => {
                        match DateTime::parse_from_rfc3339(updated_after) {
                            Ok(updated_after) => {
                                let expiration = Utc::now() - chrono::Duration::days(7);
                                updated_after.with_timezone(&Utc) < expiration
                            },
                            Err(_) => { 
                                true
                            }
                        }
                    },
                    None => {
                        true
                    }
                }
            },
            None => {
                true
            }
        };
    }
    if !load_user {
        if let Some(period_end) = users[0].data.subscription.period_ends_at {
            if period_end < Utc::now() {
                load_user = true;
            }
        }
    }

    let user = if !load_user { return Ok(users.into_iter().next().unwrap()) } 
        else { load_user_from_wk(web_config, conn, rate_limit, &user_cache).await };
    match user {
        Ok(u) => Ok(u),
        Err(e) => {
            // Fall back to the local cache of users if we had an error
            if users.len() > 0 {
                Ok(users.into_iter().next().unwrap())
            }
            else {
                Err(e)
            }
        },
    }
}

async fn load_user_from_wk(web_config: &WaniWebConfig, conn: &AsyncConnection, rate_limit: &RateLimitBox, u_cache: &Option<CacheInfo>) -> Result<wanidata::User, WaniError> {
    let headers = if let Some(u_cache) = u_cache { 
        if let Some(etag) = &u_cache.etag {
            Some(vec![(reqwest::header::IF_NONE_MATCH.to_string(), etag.to_owned())])
        } else {
            None
        }
    } else { None };
    let info = RequestInfo::<()> {
        url: "https://api.wanikani.com/v2/user".to_owned(),
        method: RequestMethod::Get,
        headers,
        ..Default::default()
    };

    match send_throttled_request(info, rate_limit.clone(), web_config.clone()).await {
        Ok((wani_resp, headers)) => {
            match wani_resp.data {
                WaniData::User(user) => {
                    let last_request_time = Utc::now();
                    let etag = headers.get(reqwest::header::ETAG);
                    let user_copy = user.clone();
                    let res = conn.call(move |conn| {
                        let res = wanisql::store_user(&user_copy, conn);
                        match res {
                            Err(e) => {
                                Err(tokio_rusqlite::Error::Other(Box::new(e)))
                            },
                            Ok(c) => Ok(c),
                        }
                    }).await;
                    if let Err(e) = res {
                        println!("Error saving user: {}", e);
                    }

                    let res = update_cache(None, CACHE_TYPE_USER, last_request_time, etag, conn).await;
                    if let Err(e) = res {
                        println!("Error updating user cache: {}", e);
                    }

                    return Ok(user)
                },
                _ => { 
                    return Err(WaniError::Generic("Unexpected response when fetching User info.".into()))
                },
            }
        },
        Err(e) => {
            return Err(e);
        },
    }
}

async fn sync_all(web_config: &WaniWebConfig, conn: &AsyncConnection, ignore_cache: bool) {
    async fn sync_subjects(conn: &AsyncConnection, 
                           web_config: &WaniWebConfig, subjects_cache: CacheInfo, rate_limit: &RateLimitBox, is_user_restricted: bool) -> Result<SyncResult, WaniError> {
        let mut next_url: Option<String> = Some("https://api.wanikani.com/v2/subjects".into());
        let mut total_parse_fails = 0;
        let mut updated_resources = 0;
        let mut headers: Option<reqwest::header::HeaderMap> = None;
        let mut last_request_time = Utc::now();
        while let Some(url) = next_url {
            let mut query: Vec<(&str, &str)> = vec![];
            if let Some(after) = &subjects_cache.updated_after {
                query.push(("updated_after", after));
            }
            if is_user_restricted {
                query.push(("levels", "1,2,3"));
            }
            let info = RequestInfo::<()> {
                url: url,
                method: RequestMethod::Get,
                query: if query.len() > 0 { Some(query) } else { None },
                headers: if let Some(tag) = &subjects_cache.last_modified {
                    Some(vec![(reqwest::header::LAST_MODIFIED.to_string(), tag.to_owned())])
                } else { None },
                ..Default::default()
            };

            last_request_time = Utc::now();
            next_url = None;
            let resp = send_throttled_request(info, rate_limit.clone(), web_config.clone()).await;
            match resp {
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
                    update_cache(Some(t.to_owned()), CACHE_TYPE_SUBJECTS, last_request_time, None, &conn).await?;
                }
                else {
                    update_cache(None, CACHE_TYPE_SUBJECTS, last_request_time, None, &conn).await?;
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

    let rate_limit = Arc::new(Mutex::new(None));
    let is_user_restricted = is_user_restricted(web_config, conn, &rate_limit).await;
    println!("Syncing subjects. . .");
    let subj_future = sync_subjects(&conn, &web_config, c_infos.remove(&CACHE_TYPE_SUBJECTS).unwrap_or(CacheInfo { id: CACHE_TYPE_SUBJECTS, ..Default::default()}), &rate_limit, is_user_restricted);
    println!("Syncing assignments. . .");
    let ass_future = sync_assignments(&conn, &web_config, c_infos.remove(&CACHE_TYPE_ASSIGNMENTS).unwrap_or(CacheInfo { id: CACHE_TYPE_ASSIGNMENTS, ..Default::default()}), &rate_limit, is_user_restricted);
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

async fn update_cache(last_modified: Option<String>, cache_type: usize, last_request_time: DateTime<Utc>, etag: Option<&HeaderValue>, conn: &AsyncConnection) -> Result<(), tokio_rusqlite::Error> {
    let last_modified = if let Some(lm) = last_modified { Some(lm.to_owned()) } else { None };
    let last_request_time = last_request_time.to_rfc3339();
    let etag = if let Some(etag) = etag { 
        if let Ok(etag) = etag.to_str() {
            Some(etag.to_owned())
        }
        else { None }
    } else { None };

    return conn.call(move |c| {
        c.execute("replace into cache_info (last_modified, updated_after, etag, id) values (?1, ?2, ?3, ?4);", params![last_modified, &last_request_time, etag, cache_type])?;
        Ok(())
    }).await;
}

fn command_init(p_config: &ProgramConfig) {
    let conn = setup_connection(&p_config);
    match conn {
        Err(e) => println!("{}", e),
        Ok(c) => {
            match setup_db(&c) {
                Ok(_) => {},
                Err(e) => {
                    println!("Error setting up SQLite DB: {}", e.to_string())
                },
            }
        },
    };
}

fn setup_db(c: &Connection) -> Result<(), SqlError> {
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

    c.execute("insert or ignore into cache_info (id) values (?1),(?2),(?3)", 
              params![
                CACHE_TYPE_SUBJECTS, 
                CACHE_TYPE_ASSIGNMENTS, 
                CACHE_TYPE_USER, 
              ])?;

    c.execute(wanisql::CREATE_REVIEWS_TBL, [])?;
    c.execute(wanisql::CREATE_RADICALS_TBL, [])?;
    c.execute(wanisql::CREATE_KANJI_TBL, [])?;
    c.execute(wanisql::CREATE_VOCAB_TBL, [])?;
    c.execute(wanisql::CREATE_KANA_VOCAB_TBL, [])?;
    c.execute(wanisql::CREATE_ASSIGNMENTS_TBL, [])?;
    c.execute(wanisql::CREATE_ASSIGNMENTS_INDEX, [])?;
    c.execute(wanisql::CREATE_USER_TBL, [])?;
    Ok(())
}

fn build_request<'a, T: serde::Serialize + Sized>(info: &RequestInfo<'a, T>, web_config: &WaniWebConfig) -> reqwest::RequestBuilder {
    let request = match info.method {
        RequestMethod::Get => web_config.client.get(info.url.clone()),
        RequestMethod::Post => web_config.client.post(info.url.clone()),
        RequestMethod::Put => web_config.client.put(info.url.clone()),
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

    if let Some(headers) = &info.headers {
        for (h, v) in headers {
            request = request.header(h.clone(), v.clone());
        }
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
            if s.is_connect() {
                Err(WaniError::Connection())
            }
            else {
                Err(WaniError::Generic(format!("Error with request: {}", s)))
            }

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
        return;
    }
    let p_config = p_config.unwrap();
    let web_config = get_web_config(&p_config);
    if let Err(e) = web_config {
        println!("{}", e);
        return;
    }
    let web_config = web_config.unwrap();

    let info = RequestInfo::<()> {
        url: "https://api.wanikani.com/v2/summary".to_owned(),
        ..Default::default()
    };

    let rate_limit = Arc::new(Mutex::new(None));
    match send_throttled_request(info, rate_limit, web_config).await {
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

fn get_image_cache(p_config: &ProgramConfig) -> Result<PathBuf, WaniError> {
    let mut db_path = get_db_path(p_config)?;
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

fn get_audio_path(p_config: &ProgramConfig) -> Result<PathBuf, WaniError> {
    let mut db_path = get_db_path(p_config)?;
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

fn get_db_path(p_config: &ProgramConfig) -> Result<PathBuf, WaniError> {
    if !Path::exists(&p_config.data_path)
    {
        if let Err(s) = fs::create_dir(&p_config.data_path) {
            return Err(WaniError::Generic(format!("Could not create datapath at {}\nError: {}", p_config.data_path.display(), s)));
        }
    }

    let mut db_path = PathBuf::from(&p_config.data_path);
    db_path.push("wani_cache.db");
    return Ok(db_path);
}

async fn setup_async_connection(p_config: &ProgramConfig) -> Result<AsyncConnection, WaniError> {
    let path = get_db_path(p_config)?;
    if !path.exists() {
        let _ = setup_connection(p_config);
    }
    let res = AsyncConnection::open(&path).await;
    Ok(res?)
}

fn setup_connection(p_config: &ProgramConfig) -> Result<Connection, WaniError> {
    let path = get_db_path(p_config)?;
    let do_init = !path.exists();
    match Connection::open(&path) {
        Ok(c) => {
            if do_init {
                match setup_db(&c) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("Error setting up SQLite DB: {}", e.to_string())
                    },
                }
            }
            Ok(c)
        },
        Err(e) => Err(WaniError::Generic(format!("{}", e))),
    }
}

fn get_program_config(args: &Args) -> Result<ProgramConfig, WaniError> {
    let mut configpath = PathBuf::new();
    if let Some(path) = &args.configfile {
        configpath.push(path);
    }
    else if let Ok(path) = std::env::var("WANI_CONFIG_PATH") {
        configpath.push(path);
    }
    else {
        match home::home_dir() {
            Some(h) => {
                configpath.push(h);
                configpath.push(".config");
                configpath.push("wani");
            },
            None => {
                return Err(WaniError::Generic(format!("Could not find home directory. Please manually specify configpath arg. Use \"wani -help\" for more details.")));
            }
        };
    }

    if !Path::exists(&configpath)
    {
        if let Err(s) = fs::create_dir(&configpath) {
            return Err(WaniError::Generic(format!("Could not create wani config folder at {}\nError: {}", configpath.display(), s)));
        }
    }
    configpath.push(".wani.conf");

    let mut auth = None;
    let mut colorblind = false;
    let mut datapath = None;
    if let Ok(lines) = read_lines(&configpath) {
        for line in lines {
            if let Ok(s) = line {
                let words = s.split(" ").collect::<Vec<&str>>();
                if words.len() < 2 {
                    continue;
                }

                match words[0] {
                    "auth:" => {
                        auth = Some(String::from(words[1]));
                    },
                    "colorblind:" => {
                        colorblind = match words[1] {
                            "true" | "True" | "t" => true,
                            _ => false,
                        };
                    },
                    "datapath:" => {
                        let path = PathBuf::from_str(words[1]);
                        if let Err(_) = path {
                            return Err(WaniError::Generic(format!("Could not parse datapath from config file. Path: {}", words[1])));
                        }
                        datapath = Some(path.unwrap());
                    }
                    _ => {},
                }
            }
        }
    }

    if let Some(a) = &args.auth {
        auth = Some(String::from(a));
    }

    let datapath = if let Some(dpath) = &args.datapath {
        dpath.clone()
    }
    else  {
        match datapath {
            Some(d) => d,
            None => {
                match home::home_dir() {
                    Some(mut h) => {
                        h.push(".wani");
                        h   
                    },
                    None => {
                        return Err(WaniError::Generic("Could not find home directory. Please manually specify datapath arg. Use \"wani -help\" for more details.".into()));
                    }
                }
            },
        }
    };

    Ok(ProgramConfig { 
        auth, 
        data_path: datapath,
        colorblind,
    })
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

async fn get_chars_for_subj(subject: &wanidata::Subject, image_cache: &PathBuf, radical_width: u32, web_config: &WaniWebConfig) -> Result<Vec<String>, WaniError> {
    Ok(match subject {
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
                        return Err(e);
                    }
                }
            };
            rad_chars
        },
        Subject::Kanji(k) => vec![k.data.characters.to_owned()],
        Subject::Vocab(v) => vec![v.data.characters.to_owned()],
        Subject::KanaVocab(kv) => vec![kv.data.characters.to_owned()],
    })
}
