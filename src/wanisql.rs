use std::{collections::HashMap, fmt::{Debug, Display}};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, Transaction};
use thiserror::Error;
use tokio_rusqlite::Connection as AsyncConnection;

use crate::wanidata::{self, AuxMeaning, ContextSentence, PronunciationAudio, VocabReading};

///! Helpers for loading/storing wanidata in Sqlite DB

#[derive(Error, Debug)]
pub(crate) enum WaniSqlError {
    Serde(#[from] serde_json::Error),
    Sql(#[from] rusqlite::Error),
    AsyncSql(#[from] tokio_rusqlite::Error),
    Chrono(#[from] chrono::ParseError),
}

impl Display for WaniSqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaniSqlError::Serde(e) => Display::fmt(&e, f),
            WaniSqlError::Sql(e) => Display::fmt(&e, f),
            WaniSqlError::AsyncSql(e) => Display::fmt(&e, f),
            WaniSqlError::Chrono(e) => Display::fmt(&e, f),
        }
    }
}

/// info for caching different WaniKani data types
#[derive(Default)]
pub(crate) struct CacheInfo {
    pub id: usize, // See CACHE_TYPE_* constants
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub updated_after: Option<String>,
}

pub const CACHE_TYPE_SUBJECTS: usize = 0;
pub const CACHE_TYPE_ASSIGNMENTS: usize = 1;
pub const CACHE_TYPE_USER: usize = 2;

pub(crate) fn setup_db(c: &Connection) -> Result<(), rusqlite::Error> {
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

    c.execute(CREATE_REVIEWS_TBL, [])?;
    c.execute(CREATE_RADICALS_TBL, [])?;
    c.execute(CREATE_KANJI_TBL, [])?;
    c.execute(CREATE_VOCAB_TBL, [])?;
    c.execute(CREATE_KANA_VOCAB_TBL, [])?;
    c.execute(CREATE_ASSIGNMENTS_TBL, [])?;
    c.execute(CREATE_ASSIGNMENTS_INDEX, [])?;
    c.execute(CREATE_USER_TBL, [])?;
    Ok(())
}

pub(crate) const CREATE_USER_TBL: &str = "create table if not exists user (
            id integer primary key,
            user text not null
        )";

pub(crate) const INSERT_USER: &str = "replace into user
                            (id, user)
                            values (1, ?1)";

pub(crate) const SELECT_USER: &str = "select * from user;";

pub(crate) fn parse_user(r: &rusqlite::Row<'_>) -> Result<wanidata::User, WaniSqlError> {
    return Ok(serde_json::from_str(&r.get::<usize, String>(1)?)?);
}

pub(crate) fn store_user(r: &wanidata::User, conn: &mut rusqlite::Connection) -> Result<usize, WaniSqlError>
{
    return Ok(conn.execute(INSERT_USER, [serde_json::to_string(r)?])?);
}

pub(crate) const CREATE_REVIEWS_TBL: &str = "create table if not exists new_reviews (
            id integer primary key,
            assignment_id integer not null,
            created_at text not null,
            incorrect_meaning_answers int not null,
            incorrect_reading_answers int not null,
            status integer not null,
            available_at text
        )";

pub(crate) const INSERT_REVIEW: &str = "replace into new_reviews
                            (id,
                             assignment_id,
                             created_at,
                             incorrect_meaning_answers,
                             incorrect_reading_answers,
                             status,
                             available_at)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

pub(crate) const INSERT_REVIEW_NO_ID: &str = "insert into new_reviews
                            (assignment_id,
                             created_at,
                             incorrect_meaning_answers,
                             incorrect_reading_answers,
                             status,
                             available_at)
                            values (?1, ?2, ?3, ?4, ?5, ?6)";

pub(crate) const SELECT_REVIEWS: &str = "select 
                            id,
                            assignment_id,
                            created_at,
                            incorrect_meaning_answers,
                            incorrect_reading_answers,
                            status,
                            available_at from new_reviews where available_at is not null;";

pub(crate) const SELECT_LESSONS: &str = "select 
                            id,
                            assignment_id,
                            created_at,
                            incorrect_meaning_answers,
                            incorrect_reading_answers,
                            status,
                            available_at from new_reviews where available_at is null;";

pub(crate) const REMOVE_REVIEW: &str = "delete from new_reviews where assignment_id = ?1;";

pub(crate) fn parse_review(r: &rusqlite::Row<'_>) -> Result<wanidata::NewReview, WaniSqlError> {
    return Ok(wanidata::NewReview {
        id: Some(r.get::<usize, i32>(0)?),
        assignment_id: r.get::<usize, i32>(1)?,
        created_at: DateTime::parse_from_rfc3339(&r.get::<usize, String>(2)?)?.with_timezone(&Utc),
        incorrect_meaning_answers: r.get::<usize, u16>(3)?,
        incorrect_reading_answers: r.get::<usize, u16>(4)?,
        status: wanidata::ReviewStatus::from(r.get::<usize, usize>(5)?),
        available_at: 
            if let Some(t) = r.get::<usize, Option<String>>(6)? { 
                Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
            } 
            else { 
                None 
            },
    });
}

pub(crate) fn store_review(r: &wanidata::NewReview, stmt: &mut Transaction<'_>) -> Result<usize, rusqlite::Error>
{
    let status: usize = r.status.into();
    if let Some(id) = r.id {
        let p = rusqlite::params!(
            id,
            r.assignment_id,
            r.created_at.to_rfc3339(),
            r.incorrect_meaning_answers,
            r.incorrect_reading_answers,
            status,
            if let Some(available_at) = r.available_at { Some(available_at.to_rfc3339()) } else { None },
            );
        return stmt.execute(INSERT_REVIEW, p);
    }
    else {
        let p = rusqlite::params!(
            r.assignment_id,
            r.created_at.to_rfc3339(),
            r.incorrect_meaning_answers,
            r.incorrect_reading_answers,
            status,
            if let Some(available_at) = r.available_at { Some(available_at.to_rfc3339()) } else { None },
            );
        return stmt.execute(INSERT_REVIEW_NO_ID, p);
    }
}

pub(crate) const CREATE_ASSIGNMENTS_TBL: &str = "create table if not exists assignments (
            id integer primary key,
            available_at int,
            created_at text not null,
            hidden integer not null,
            srs_stage integer not null,
            started_at text,
            subject_id integer not null,
            subject_type integer not null,
            unlocked_at text
        )";

pub(crate) const CREATE_ASSIGNMENTS_INDEX: &str = 
    "create index if not exists idx_available_at 
        on assignments (available_at);";

pub(crate) const INSERT_ASSIGNMENT: &str = "replace into assignments
                            (id,
                             available_at,
                             created_at,
                             hidden,
                             srs_stage,
                             started_at,
                             subject_id,
                             subject_type,
                             unlocked_at)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

pub(crate) const SELECT_LESSON_ASSIGNMENTS: &str = "select 
                            id,
                            available_at,
                            created_at,
                            hidden,
                            srs_stage,
                            started_at,
                            subject_id,
                            subject_type from assignments 
                        where started_at is null and unlocked_at is not null;";

pub(crate) const SELECT_AVAILABLE_ASSIGNMENTS: &str = "select 
                            id,
                            available_at,
                            created_at,
                            hidden,
                            srs_stage,
                            started_at,
                            subject_id,
                            subject_type from assignments 
                        where available_at < ?1;";// and started_at is not null;";

pub(crate) fn parse_assignment(r: &rusqlite::Row<'_>) -> Result<wanidata::Assignment, WaniSqlError> {
    return Ok(wanidata::Assignment {
        id: r.get::<usize, i32>(0)?,
        data: wanidata::AssignmentData { 
            available_at: 
                if let Some(t) = r.get::<usize, Option<i64>>(1)? { 
                    match Utc.timestamp_opt(t, 0) {
                        chrono::LocalResult::None => None,
                        chrono::LocalResult::Single(s) => Some(s),
                        chrono::LocalResult::Ambiguous(_, _) => None,
                    }
                }
                else { 
                    None 
                },
            created_at: DateTime::parse_from_rfc3339(&r.get::<usize, String>(2)?)?.with_timezone(&Utc),
            hidden: r.get::<usize, bool>(3)?,
            srs_stage: r.get::<usize, i32>(4)?,
            started_at: 
                if let Some(t) = r.get::<usize, Option<String>>(5)? { 
                    Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
                } 
                else { 
                    None 
                },
            subject_id: r.get::<usize, i32>(6)?,
            subject_type: wanidata::SubjectType::from(r.get::<usize, usize>(7)?),
            unlocked_at: None,
        }
    });
}

pub(crate) fn store_assignment(r: wanidata::Assignment, stmt: &mut Transaction<'_>) -> Result<usize, rusqlite::Error>
{
    let subj_type: usize = r.data.subject_type.into();
    let p = rusqlite::params!(
        format!("{}", r.id),
        if let Some(available_at) = r.data.available_at { Some(available_at.timestamp()) } else { None },
        r.data.created_at.to_rfc3339(),
        r.data.hidden,
        r.data.srs_stage,
        if let Some(started_at) = r.data.started_at { Some(started_at.to_rfc3339()) } else { None },
        r.data.subject_id,
        subj_type,
        if let Some(unlocked_at) = r.data.unlocked_at { Some(unlocked_at.to_rfc3339()) } else { None },
        );
    return stmt.execute(INSERT_ASSIGNMENT, p);
}

pub(crate) const CREATE_RADICALS_TBL: &str = "create table if not exists radicals (
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
        )";

pub(crate) const INSERT_RADICALS: &str = "replace into radicals
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

pub(crate) fn select_radicals_by_id(n: usize) -> String {
    return format!("select 
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
                   character_images from radicals
                   where id in ({});",
        std::iter::repeat("?").take(n).collect::<Vec<_>>().join(","));
}

pub(crate) fn store_radical(r: wanidata::Radical, stmt: &mut Transaction<'_>) -> Result<usize, WaniSqlError>
{
    let p = rusqlite::params!(
        format!("{}", r.id),
        serde_json::to_string(&r.data.aux_meanings)?,
        r.data.created_at.to_rfc3339(),
        r.data.document_url,
        if let Some(hidden_at) = r.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", r.data.lesson_position),
        format!("{}", r.data.level),
        r.data.meaning_mnemonic,
        serde_json::to_string(&r.data.meanings)?,
        r.data.slug,
        format!("{}", r.data.spaced_repetition_system_id),
        serde_json::to_string(&r.data.amalgamation_subject_ids)?,
        if let Some(chars) = r.data.characters { Some(chars) } else { None },
        serde_json::to_string(&r.data.character_images)?,
        );

    match stmt.execute(INSERT_RADICALS, p) {
        Ok(u) => Ok(u),
        Err(e) => Err(WaniSqlError::Sql(e)),
    }
}

pub(crate) fn parse_radical(r: &rusqlite::Row<'_>) -> Result<wanidata::Radical, WaniSqlError> {
    return Ok(wanidata::Radical {
        id: r.get::<usize, i32>(0)?,
        data: wanidata::RadicalData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&r.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&r.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: r.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = r.get::<usize, Option<String>>(4)? { 
                    Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
                } 
                else { 
                    None 
                },
                lesson_position: r.get::<usize, i32>(5)?, 
                level: r.get::<usize, i32>(6)?, 
                meaning_mnemonic: r.get::<usize, String>(7)?, 
                meanings: serde_json::from_str::<Vec<wanidata::Meaning>>(&r.get::<usize, String>(8)?)?, 
                slug: r.get::<usize, String>(9)?, 
                spaced_repetition_system_id: r.get::<usize, i32>(10)?, 
                amalgamation_subject_ids: serde_json::from_str::<Vec<i32>>(&r.get::<usize, String>(11)?)?, 
                characters: r.get::<usize, Option<String>>(12)?, 
                character_images: serde_json::from_str::<Vec<wanidata::RadicalImage>>(&r.get::<usize, String>(13)?)?, 
        }
    });
}

pub(crate) const CREATE_KANJI_TBL: &str = "create table if not exists kanji (
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
        )";

pub(crate) const INSERT_KANJI: &str = "replace into kanji
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
                             characters,
                             amalgamation_subject_ids,
                             component_subject_ids,
                             meaning_hint,
                             reading_hint,
                             reading_mnemonic,
                             readings,
                             visually_similar_subject_ids)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)";

pub(crate) fn select_kanji_by_id(n: usize) -> String {
    return format!("select id,
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
                             characters,
                             amalgamation_subject_ids,
                             component_subject_ids,
                             meaning_hint,
                             reading_hint,
                             reading_mnemonic,
                             readings,
                             visually_similar_subject_ids from kanji
                             where id in ({});",
        std::iter::repeat("?").take(n).collect::<Vec<_>>().join(","));
}

pub(crate) fn store_kanji(k: wanidata::Kanji, stmt: &mut Transaction<'_>) -> Result<usize, WaniSqlError>
{
    let p = rusqlite::params!(
        format!("{}", k.id),
        serde_json::to_string(&k.data.aux_meanings)?,
        k.data.created_at.to_rfc3339(),
        k.data.document_url,
        if let Some(hidden_at) = k.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", k.data.lesson_position),
        format!("{}", k.data.level),
        k.data.meaning_mnemonic,
        serde_json::to_string(&k.data.meanings)?,
        k.data.slug,
        format!("{}", k.data.spaced_repetition_system_id),
        k.data.characters,
        serde_json::to_string(&k.data.amalgamation_subject_ids)?,
        serde_json::to_string(&k.data.component_subject_ids)?,
        k.data.meaning_hint,
        k.data.reading_hint,
        k.data.reading_mnemonic,
        serde_json::to_string(&k.data.readings)?,
        serde_json::to_string(&k.data.visually_similar_subject_ids)?,
        );

    match stmt.execute(INSERT_KANJI, p) {
        Ok(u) => Ok(u),
        Err(e) => Err(WaniSqlError::Sql(e)),
    }
}

pub(crate) fn parse_kanji(k: &rusqlite::Row<'_>) -> Result<wanidata::Kanji, WaniSqlError> {
    return Ok(wanidata::Kanji {
        id: k.get::<usize, i32>(0)?,
        data: wanidata::KanjiData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&k.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&k.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: k.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = k.get::<usize, Option<String>>(4)? { 
                    Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
                } 
                else { 
                    None 
                },
            lesson_position: k.get::<usize, i32>(5)?, 
            level: k.get::<usize, i32>(6)?, 
            meaning_mnemonic: k.get::<usize, String>(7)?, 
            meanings: serde_json::from_str::<Vec<wanidata::Meaning>>(&k.get::<usize, String>(8)?)?, 
            slug: k.get::<usize, String>(9)?, 
            spaced_repetition_system_id: k.get::<usize, i32>(10)?, 
            characters: k.get::<usize, String>(11)?, 
            amalgamation_subject_ids: serde_json::from_str::<Vec<i32>>(&k.get::<usize, String>(12)?)?, 
            component_subject_ids: serde_json::from_str::<Vec<i32>>(&k.get::<usize, String>(13)?)?, 
            meaning_hint: k.get::<usize, Option<String>>(14)?,
            reading_hint: k.get::<usize, Option<String>>(15)?,
            reading_mnemonic: k.get::<usize, String>(16)?,
            readings: serde_json::from_str::<Vec<wanidata::KanjiReading>>(&k.get::<usize, String>(17)?)?,
            visually_similar_subject_ids: serde_json::from_str::<Vec<i32>>(&k.get::<usize, String>(18)?)?, 
        }
    });
}

pub(crate) const CREATE_VOCAB_TBL: &str = "create table if not exists vocab (
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
        )";

pub(crate) const INSERT_VOCAB: &str = "replace into vocab
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
                             characters,
                             component_subject_ids,
                             context_sentences,
                             parts_of_speech,
                             pronunciation_audios,
                             readings,
                             reading_mnemonic)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)";

pub(crate) fn select_vocab_by_id(n: usize) -> String {
    return format!("select id,
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
                             characters,
                             component_subject_ids,
                             context_sentences,
                             parts_of_speech,
                             pronunciation_audios,
                             readings,
                             reading_mnemonic from vocab
                             where id in ({});",
        std::iter::repeat("?").take(n).collect::<Vec<_>>().join(","));
}

pub(crate) fn store_vocab(v: wanidata::Vocab, stmt: &mut Transaction<'_>) -> Result<usize, WaniSqlError>
{
    let p = rusqlite::params!(
        format!("{}", v.id),
        serde_json::to_string(&v.data.aux_meanings)?,
        v.data.created_at.to_rfc3339(),
        v.data.document_url,
        if let Some(hidden_at) = v.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", v.data.lesson_position),
        format!("{}", v.data.level),
        v.data.meaning_mnemonic,
        serde_json::to_string(&v.data.meanings)?,
        v.data.slug,
        format!("{}", v.data.spaced_repetition_system_id),
        v.data.characters,
        serde_json::to_string(&v.data.component_subject_ids)?,
        serde_json::to_string(&v.data.context_sentences)?,
        serde_json::to_string(&v.data.parts_of_speech)?,
        serde_json::to_string(&v.data.pronunciation_audios)?,
        serde_json::to_string(&v.data.readings)?,
        v.data.reading_mnemonic
        );

    match stmt.execute(INSERT_VOCAB, p) {
        Ok(u) => Ok(u),
        Err(e) => Err(WaniSqlError::Sql(e)),
    }
}

pub(crate) fn parse_vocab(v: &rusqlite::Row<'_>) -> Result<wanidata::Vocab, WaniSqlError> {
    return Ok(wanidata::Vocab {
        id: v.get::<usize, i32>(0)?,
        data: wanidata::VocabData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&v.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&v.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: v.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = v.get::<usize, Option<String>>(4)? { 
                    Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
                } 
                else { 
                    None 
                },
            lesson_position: v.get::<usize, i32>(5)?, 
            level: v.get::<usize, i32>(6)?, 
            meaning_mnemonic: v.get::<usize, String>(7)?, 
            meanings: serde_json::from_str::<Vec<wanidata::Meaning>>(&v.get::<usize, String>(8)?)?, 
            slug: v.get::<usize, String>(9)?, 
            spaced_repetition_system_id: v.get::<usize, i32>(10)?, 
            characters: v.get::<usize, String>(11)?, 
            component_subject_ids: serde_json::from_str::<Vec<i32>>(&v.get::<usize, String>(12)?)?, 
            context_sentences: serde_json::from_str::<Vec<ContextSentence>>(&v.get::<usize, String>(13)?)?, 
            parts_of_speech: serde_json::from_str::<Vec<String>>(&v.get::<usize, String>(14)?)?, 
            pronunciation_audios: serde_json::from_str::<Vec<PronunciationAudio>>(&v.get::<usize, String>(15)?)?, 
            readings: serde_json::from_str::<Vec<VocabReading>>(&v.get::<usize, String>(16)?)?, 
            reading_mnemonic: v.get::<usize, String>(17)?
        }
    });
}

pub(crate) const CREATE_KANA_VOCAB_TBL: &str = "create table if not exists kana_vocab (
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
        )";

pub(crate) const INSERT_KANA_VOCAB: &str = "replace into kana_vocab
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
                             characters,
                             context_sentences,
                             parts_of_speech,
                             pronunciation_audios)
                            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)";

pub(crate) fn store_kana_vocab(v: wanidata::KanaVocab, stmt: &mut Transaction<'_>) -> Result<usize, WaniSqlError>
{
    let p = rusqlite::params!(
        format!("{}", v.id),
        serde_json::to_string(&v.data.aux_meanings)?,
        v.data.created_at.to_rfc3339(),
        v.data.document_url,
        if let Some(hidden_at) = v.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", v.data.lesson_position),
        format!("{}", v.data.level),
        v.data.meaning_mnemonic,
        serde_json::to_string(&v.data.meanings)?,
        v.data.slug,
        format!("{}", v.data.spaced_repetition_system_id),
        v.data.characters,
        serde_json::to_string(&v.data.context_sentences)?,
        serde_json::to_string(&v.data.parts_of_speech)?,
        serde_json::to_string(&v.data.pronunciation_audios)?
        );

    match stmt.execute(INSERT_KANA_VOCAB, p) {
        Ok(u) => Ok(u),
        Err(e) => Err(WaniSqlError::Sql(e)),
    }
}

pub(crate) fn select_kana_vocab_by_id(n: usize) -> String {
    return format!("select id,
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
                   characters,
                   context_sentences,
                   parts_of_speech,
                   pronunciation_audios from kana_vocab
                   where id in ({});",
                         std::iter::repeat("?").take(n).collect::<Vec<_>>().join(","));
}

pub(crate) fn parse_kana_vocab(v: &rusqlite::Row<'_>) -> Result<wanidata::KanaVocab, WaniSqlError> {
    return Ok(wanidata::KanaVocab {
        id: v.get::<usize, i32>(0)?,
        data: wanidata::KanaVocabData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&v.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&v.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: v.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = v.get::<usize, Option<String>>(4)? { 
                    Some(DateTime::parse_from_rfc3339(&t)?.with_timezone(&Utc))
                } 
                else { 
                    None 
                },
            lesson_position: v.get::<usize, i32>(5)?, 
            level: v.get::<usize, i32>(6)?, 
            meaning_mnemonic: v.get::<usize, String>(7)?, 
            meanings: serde_json::from_str::<Vec<wanidata::Meaning>>(&v.get::<usize, String>(8)?)?, 
            slug: v.get::<usize, String>(9)?, 
            spaced_repetition_system_id: v.get::<usize, i32>(10)?, 
            characters: v.get::<usize, String>(11)?, 
            context_sentences: serde_json::from_str::<Vec<ContextSentence>>(&v.get::<usize, String>(12)?)?, 
            parts_of_speech: serde_json::from_str::<Vec<String>>(&v.get::<usize, String>(13)?)?, 
            pronunciation_audios: serde_json::from_str::<Vec<PronunciationAudio>>(&v.get::<usize, String>(14)?)?, 
        }
    });
}

pub(crate) async fn get_all_cache_infos(conn: &AsyncConnection, ignore_cache: bool) -> Result<HashMap<usize, CacheInfo>, WaniSqlError> {
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
