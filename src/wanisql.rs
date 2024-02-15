use chrono::{DateTime, Utc};
use rusqlite::Statement;

use crate::{wanidata::{self, AuxMeaning, ContextSentence, PronunciationAudio, VocabReading}, WaniError};

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

pub(crate) const SELECT_ALL_RADICALS: &str = "select 
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
                                      character_images from radicals;";

pub(crate) fn store_radical(r: wanidata::Radical, stmt: &mut Statement<'_>) -> Result<usize, rusqlite::Error>
{
    let p = rusqlite::params!(
        format!("{}", r.id),
        serde_json::to_string(&r.data.aux_meanings).unwrap(),
        r.data.created_at.to_rfc3339(),
        r.data.document_url,
        if let Some(hidden_at) = r.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", r.data.lesson_position),
        format!("{}", r.data.level),
        r.data.meaning_mnemonic,
        serde_json::to_string(&r.data.meanings).unwrap(),
        r.data.slug,
        format!("{}", r.data.spaced_repetition_system_id),
        serde_json::to_string(&r.data.amalgamation_subject_ids).unwrap(),
        if let Some(chars) = r.data.characters { Some(chars) } else { None },
        serde_json::to_string(&r.data.character_images).unwrap(),
        );
    return stmt.execute(p);
}

pub(crate) fn parse_radical(r: &rusqlite::Row<'_>) -> Result<wanidata::Radical, WaniError> {
    return Ok(wanidata::Radical {
        id: r.get::<usize, i32>(0)?,
        data: wanidata::RadicalData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&r.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&r.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: r.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = r.get::<usize, Option<String>>(4)? { 
                    println!("Hidden at: {}", t);
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

pub(crate) const SELECT_ALL_KANJI: &str = "select id,
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
                             visually_similar_subject_ids from kanji;";

pub(crate) fn store_kanji(k: wanidata::Kanji, stmt: &mut Statement<'_>) -> Result<usize, rusqlite::Error>
{
    let p = rusqlite::params!(
        format!("{}", k.id),
        serde_json::to_string(&k.data.aux_meanings).unwrap(),
        k.data.created_at.to_rfc3339(),
        k.data.document_url,
        if let Some(hidden_at) = k.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", k.data.lesson_position),
        format!("{}", k.data.level),
        k.data.meaning_mnemonic,
        serde_json::to_string(&k.data.meanings).unwrap(),
        k.data.slug,
        format!("{}", k.data.spaced_repetition_system_id),
        k.data.characters,
        serde_json::to_string(&k.data.amalgamation_subject_ids).unwrap(),
        serde_json::to_string(&k.data.component_subject_ids).unwrap(),
        k.data.meaning_hint,
        k.data.reading_hint,
        k.data.reading_mnemonic,
        serde_json::to_string(&k.data.readings).unwrap(),
        serde_json::to_string(&k.data.visually_similar_subject_ids).unwrap(),
        );
    return stmt.execute(p);
}

pub(crate) fn parse_kanji(k: &rusqlite::Row<'_>) -> Result<wanidata::Kanji, WaniError> {
    return Ok(wanidata::Kanji {
        id: k.get::<usize, i32>(0)?,
        data: wanidata::KanjiData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&k.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&k.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: k.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = k.get::<usize, Option<String>>(4)? { 
                    println!("Hidden at: {}", t);
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

pub(crate) const SELECT_ALL_VOCAB: &str = "select id,
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
                             reading_mnemonic from vocab;";

pub(crate) fn store_vocab(v: wanidata::Vocab, stmt: &mut Statement<'_>) -> Result<usize, rusqlite::Error>
{
    let p = rusqlite::params!(
        format!("{}", v.id),
        serde_json::to_string(&v.data.aux_meanings).unwrap(),
        v.data.created_at.to_rfc3339(),
        v.data.document_url,
        if let Some(hidden_at) = v.data.hidden_at { Some(hidden_at.to_rfc3339()) } else { None },
        format!("{}", v.data.lesson_position),
        format!("{}", v.data.level),
        v.data.meaning_mnemonic,
        serde_json::to_string(&v.data.meanings).unwrap(),
        v.data.slug,
        format!("{}", v.data.spaced_repetition_system_id),
        v.data.characters,
        serde_json::to_string(&v.data.component_subject_ids).unwrap(),
        serde_json::to_string(&v.data.context_sentences).unwrap(),
        serde_json::to_string(&v.data.parts_of_speech).unwrap(),
        serde_json::to_string(&v.data.pronunciation_audios).unwrap(),
        serde_json::to_string(&v.data.readings).unwrap(),
        v.data.reading_mnemonic
        );
    return stmt.execute(p);
}

pub(crate) fn parse_vocab(v: &rusqlite::Row<'_>) -> Result<wanidata::Vocab, WaniError> {
    return Ok(wanidata::Vocab {
        id: v.get::<usize, i32>(0)?,
        data: wanidata::VocabData { 
            aux_meanings: serde_json::from_str::<Vec<AuxMeaning>>(&v.get::<usize, String>(1)?)?,
            created_at: DateTime::parse_from_rfc3339(&v.get::<usize, String>(2)?)?.with_timezone(&Utc),
            document_url: v.get::<usize, String>(3)?, 
            hidden_at: 
                if let Some(t) = v.get::<usize, Option<String>>(4)? { 
                    println!("Hidden at: {}", t);
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
