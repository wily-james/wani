use serde::{Deserialize, Serialize};
use chrono::{
    DateTime,
    Utc,
};

#[derive(Debug, Deserialize)]
pub struct WaniResp {
    pub url: String,
    pub data_updated_at: Option<String>, // TODO - optional for collections if no elements, mandatory
    #[serde(flatten)]
    pub data: WaniData
}

#[derive(Deserialize, Debug)]
#[serde(tag="object")]
pub enum WaniData
{
    #[serde(rename="collection")]
    Collection(Collection),
    #[serde(rename="report")]
    Report(Summary),

    // Resources:
    #[serde(rename="assignment")]
    Assignment(Assignment),
    #[serde(rename="kana_vocabulary")]
    KanaVocabulary(KanaVocab),
    #[serde(rename="kanji")]
    Kanji(Kanji),
    #[serde(rename="level_progression")]
    LevelProgression,
    #[serde(rename="radical")]
    Radical(Radical),
    #[serde(rename="reset")]
    Reset,
    #[serde(rename="review_statistic")]
    ReviewStatistic,
    #[serde(rename="review")]
    Review(Review),
    #[serde(rename="spaced_repetition_system")]
    SpacedRepetitionSystem,
    #[serde(rename="study_material")]
    StudyMaterial,
    #[serde(rename="user")]
    User,
    #[serde(rename="vocabulary")]
    Vocabulary(Vocab),
    #[serde(rename="voice_actor")]
    VoiceActor,
}

pub enum Subject
{
    Radical(Radical),
    Kanji(Kanji),
    Vocab(Vocab),
    KanaVocab(KanaVocab),
}

#[derive(Deserialize, Debug)]
pub struct Assignment {
    pub id: i32,
    pub data: AssignmentData,
}

#[derive(Deserialize, Debug)]
pub struct AssignmentData {
    pub available_at: Option<DateTime<Utc>>,
    pub burned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub hidden: bool,
    pub passed_at: Option<DateTime<Utc>>,
    pub resurrected_at: Option<DateTime<Utc>>,
    pub srs_stage: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub subject_id: i32,
    pub subject_type: SubjectType,
    pub unlocked_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Debug)]
pub struct Review {
    pub id: i32,
    pub assignment_id: i32,
    pub created_at: DateTime<Utc>,
    pub ending_srs_stage: u8,
    pub incorrect_meaning_answers: u16,
    pub incorrect_reading_answers: u16,
    pub spaced_repetition_system_id: i32,
    pub starting_srs_stage: u8,
    pub subject_id: i32
}

#[derive(Deserialize, Debug)]
pub struct NewReview {
    pub assignment_id: i32,
    pub created_at: DateTime<Utc>,
    pub incorrect_meaning_answers: u16,
    pub incorrect_reading_answers: u16,
    pub status: ReviewStatus,
}

#[derive(Deserialize, Debug)]
pub enum ReviewStatus {
    NotStarted,
    MeaningDone,
    ReadingDone,
    Done
}

#[derive(Deserialize, Debug)]
pub enum SubjectType {
    #[serde(rename="radical")]
    Radical,
    #[serde(rename="kanji")]
    Kanji,
    #[serde(rename="vocabulary")]
    Vocab,
    #[serde(rename="kana_vocabulary")]
    KanaVocab
}


impl std::convert::Into<usize> for SubjectType {
    fn into(self) -> usize {
        match self {
            SubjectType::Radical => 0,
            SubjectType::Kanji => 1,
            SubjectType::Vocab => 2,
            SubjectType::KanaVocab => 3,
        }
    }
}

impl std::convert::From<usize> for SubjectType {
    fn from(value: usize) -> Self {
        match value {
            0 => SubjectType::Radical,
            1 => SubjectType::Kanji,
            2 => SubjectType::Vocab,
            3 => SubjectType::KanaVocab,
            _ => panic!(),
        }
    }
}


#[derive(Deserialize, Debug)]
pub struct Collection {
    pub data: Vec<WaniData>,
    pub pages: PageData
}

#[derive(Deserialize, Debug)]
pub struct PageData {
    pub per_page: i32,
    pub next_url: Option<String>,
    pub previous_url: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Radical {
    // Resource Common
    pub id: i32,

    pub data: RadicalData,
}

#[derive(Deserialize, Debug)]
pub struct RadicalData {
    // Subject Common
    #[serde(rename="auxiliary_meanings")]
    pub aux_meanings: Vec<AuxMeaning>,
    pub created_at: DateTime<Utc>,
    pub document_url: String,
    pub hidden_at: Option<DateTime<Utc>>,
    pub lesson_position: i32,
    pub level: i32,
    pub meaning_mnemonic: String,
    pub meanings: Vec<Meaning>,
    pub slug: String,
    pub spaced_repetition_system_id: i32,

    // Radical Specific
    pub amalgamation_subject_ids: Vec<i32>,
    pub characters: Option<String>,
    pub character_images: Vec<RadicalImage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RadicalImage
{
    pub url: String
}


#[derive(Deserialize, Debug)]
pub struct Kanji {
    // Resource Common
    pub id: i32,

    pub data: KanjiData,
}

#[derive(Deserialize, Debug)]
pub struct KanjiData {
    // Subject Common
    #[serde(rename="auxiliary_meanings")]
    pub aux_meanings: Vec<AuxMeaning>,
    pub created_at: DateTime<Utc>,
    pub document_url: String,
    pub hidden_at: Option<DateTime<Utc>>,
    pub lesson_position: i32,
    pub level: i32,
    pub meaning_mnemonic: String,
    pub meanings: Vec<Meaning>,
    pub slug: String,
    pub spaced_repetition_system_id: i32,

    // Kanji-Specific
    pub characters: String,
    pub amalgamation_subject_ids: Vec<i32>,
    pub component_subject_ids: Vec<i32>,
    pub meaning_hint: Option<String>,
    pub reading_hint: Option<String>,
    pub reading_mnemonic: String,
    pub readings: Vec<KanjiReading>,
    pub visually_similar_subject_ids: Vec<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KanjiReading {
    pub reading: String,
    pub primary: bool,
    pub accepted_answer: bool,
    pub r#type: KanjiType,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum KanjiType
{
    #[serde(rename="kunyomi")]
    Kunyomi,
    #[serde(rename="nanori")]
    Nanori,
    #[serde(rename="onyomi")]
    Onyomi
}

#[derive(Deserialize, Debug)]
pub struct Vocab
{
    // Resource Common
    pub id: i32,

    pub data: VocabData
}

#[derive(Deserialize, Debug)]
pub struct VocabData
{
    // Subject Common
    #[serde(rename="auxiliary_meanings")]
    pub aux_meanings: Vec<AuxMeaning>,
    pub created_at: DateTime<Utc>,
    pub document_url: String,
    pub hidden_at: Option<DateTime<Utc>>,
    pub lesson_position: i32,
    pub level: i32,
    pub meaning_mnemonic: String,
    pub meanings: Vec<Meaning>,
    pub slug: String,
    pub spaced_repetition_system_id: i32,

    pub characters: String,
    pub component_subject_ids: Vec<i32>,
    pub context_sentences: Vec<ContextSentence>,
    pub parts_of_speech: Vec<String>,
    pub pronunciation_audios: Vec<PronunciationAudio>,
    pub readings: Vec<VocabReading>,
    pub reading_mnemonic: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContextSentence {
    pub en: String,
    pub ja: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PronunciationAudio {
    pub url: String,
    pub content_type: String,
    pub metadata: PronunciationMetadata
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PronunciationMetadata
{
    pub gender: String,
    pub source_id: i32,
    pub pronunciation: String,
    pub voice_actor_id: i32,
    pub voice_actor_name: String,
    pub voice_description: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VocabReading {
    pub accepted_answer: bool,
    pub primary: bool,
    pub reading: String,
}

#[derive(Deserialize, Debug)]
pub struct KanaVocab {
    // Resource Common
    pub id: i32,

    pub data: KanaVocabData
}

#[derive(Deserialize, Debug)]
pub struct KanaVocabData {
    // Subject Common
    #[serde(rename="auxiliary_meanings")]
    pub aux_meanings: Vec<AuxMeaning>,
    pub created_at: DateTime<Utc>,
    pub document_url: String,
    pub hidden_at: Option<DateTime<Utc>>,
    pub lesson_position: i32,
    pub level: i32,
    pub meaning_mnemonic: String,
    pub meanings: Vec<Meaning>,
    pub slug: String,
    pub spaced_repetition_system_id: i32,

    pub characters: String,
    pub context_sentences: Vec<ContextSentence>,
    pub parts_of_speech: Vec<String>,
    pub pronunciation_audios: Vec<PronunciationAudio>,
}

#[derive(Deserialize, Debug)]
pub struct Summary {
    pub data: SummaryData
}

#[derive(Deserialize, Debug)]
pub struct SummaryData {
    pub lessons: Vec<Lesson>,
    //next_reviews_at: Option<String>,
    pub reviews: Vec<SummaryReview>
}

#[derive(Deserialize, Debug)]
pub struct SummaryReview {
    pub available_at: DateTime<Utc>,
    pub subject_ids: Vec<i32>,
}

#[derive(Deserialize, Debug)]
pub struct Lesson {
    pub available_at: DateTime<Utc>,
    pub subject_ids: Vec<i32>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meaning {
    pub meaning: String,
    pub primary: bool,
    pub accepted_answer: bool,
}

pub enum AnswerResult {
    Correct,
    Incorrect,

    // Correct, but needed to fuzzy-match to the correct answer
    FuzzyCorrect,

    // This is an answer, but not an accepted answer
    MatchesNonAcceptedAnswer,

    /// Entered kana when meaning was expected
    KanaWhenMeaning,

    // Input contains illegal characters
    BadFormatting,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuxMeaning {
    pub r#type: AuxMeaningType,
    pub meaning: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuxMeaningType
{
    #[serde(rename="whitelist")]
    Whitelist,
    #[serde(rename="blacklist")]
    Blacklist
}

pub fn is_correct_answer(subject: &Subject, guess: &str, is_meaning: bool, kana_input: &str) -> AnswerResult {
    match subject {
        Subject::KanaVocab(v) => is_meaning_correct(&v.data.meanings, &guess),
        Subject::Radical(r) => is_meaning_correct(&r.data.meanings, &guess),
        Subject::Kanji(k) => {
            if is_meaning {
                let best = is_meaning_correct(&k.data.meanings, &guess);
                if let AnswerResult::Correct = best {
                    return best;
                }

                if let AnswerResult::Correct = is_reading_correct(&k.data.readings, kana_input) {
                    return AnswerResult::KanaWhenMeaning;
                }

                return best;
            }
            else {
                is_reading_correct(&k.data.readings, &guess)
            }
        },
        Subject::Vocab(v) => {
            if is_meaning {
                let best = is_meaning_correct(&v.data.meanings, &guess);
                if let AnswerResult::Correct = best {
                    return best;
                }

                if let AnswerResult::Correct = is_vocab_reading_correct(&v.data.readings, kana_input) {
                    return AnswerResult::KanaWhenMeaning;
                }

                return best;
            }
            else {
                is_vocab_reading_correct(&v.data.readings, &guess)
            }
        },
    }
}

pub fn is_vocab_reading_correct(readings: &Vec<VocabReading>, guess: &str) -> AnswerResult {
    for reading in readings {
        if reading.reading.trim().to_lowercase() == guess {
            if reading.accepted_answer {
                return AnswerResult::Correct;
            }

            return AnswerResult::MatchesNonAcceptedAnswer;
        }
    }

    return AnswerResult::Incorrect;
}

pub fn is_reading_correct(readings: &Vec<KanjiReading>, guess: &str) -> AnswerResult {
    for reading in readings {
        if guess == reading.reading.trim().to_lowercase() {
            if reading.accepted_answer {
                return AnswerResult::Correct;
            }

            return AnswerResult::MatchesNonAcceptedAnswer;
        }
    }

    return AnswerResult::Incorrect;
}

pub fn is_meaning_correct(meanings: &Vec<Meaning>, guess: &str) -> AnswerResult {
    let mut best = AnswerResult::Incorrect;
    for meaning in meanings {
        if guess == meaning.meaning.trim().to_lowercase() {
            if meaning.accepted_answer {
                return AnswerResult::Correct;
            }

            best = AnswerResult::MatchesNonAcceptedAnswer;
        }
    }

    return best;
}

pub struct WaniFmtArgs<'a> {
    pub radical_args: WaniTagArgs<'a>,
    pub kanji_args: WaniTagArgs<'a>,
    pub vocab_args: WaniTagArgs<'a>,
    pub meaning_args: WaniTagArgs<'a>,
    pub reading_args: WaniTagArgs<'a>,
    pub ja_args: WaniTagArgs<'a>,
}

pub struct WaniTagArgs<'a> {
    pub open_tag: &'a str,
    pub close_tag: &'a str,
}

pub const EMPTY_ARGS: WaniFmtArgs = WaniFmtArgs {
    radical_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
    kanji_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
    vocab_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
    meaning_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
    reading_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
    ja_args: WaniTagArgs { 
        open_tag: "",
        close_tag: "",
    },
};

pub fn format_wani_text(s: &str, args: &WaniFmtArgs) -> String {
    let s = s.replace("<radical>", args.radical_args.open_tag);
    let s = s.replace("</radical>", args.radical_args.close_tag);
    let s = s.replace("<kanji>", args.kanji_args.open_tag);
    let s = s.replace("</kanji>", args.kanji_args.close_tag);
    let s = s.replace("<vocabulary>", args.vocab_args.open_tag);
    let s = s.replace("</vocabulary>", args.vocab_args.close_tag);
    let s = s.replace("<reading>", args.reading_args.open_tag);
    let s = s.replace("</reading>", args.reading_args.close_tag);
    let s = s.replace("<ja>", args.ja_args.open_tag);
    let s = s.replace("</ja>", args.ja_args.close_tag);
    let s = s.replace("<meaning>", args.meaning_args.open_tag);
    s.replace("</meaning>", args.meaning_args.close_tag)
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use chrono::Utc;

    use crate::wanidata::AnswerResult;

    use super::{format_wani_text, is_correct_answer, AuxMeaning, Kanji, KanjiData, KanjiReading, Meaning, Subject, WaniFmtArgs, EMPTY_ARGS};

    fn get_kanji(meanings: Vec<Meaning>, readings: Vec<KanjiReading>, aux_meanings: Vec<AuxMeaning>) -> Kanji {
        Kanji {
            id: 1,
            data: KanjiData {
                aux_meanings,
                readings,
                meanings,
                created_at: Utc::now(),
                document_url: "".into(),
                hidden_at: None,
                lesson_position: 1,
                level: 1,
                meaning_mnemonic: "".into(),
                slug: "".into(),
                spaced_repetition_system_id: 1,
                characters: "".into(),
                amalgamation_subject_ids: vec![],
                component_subject_ids: vec![],
                meaning_hint: None,
                reading_hint: None,
                reading_mnemonic: "".into(),
                visually_similar_subject_ids: vec![],
            },
        }
    }

    fn get_standard_kanji() -> Kanji {
        let meanings = vec![
            Meaning {
                meaning: "not_accepted".into(),
                primary: false,
                accepted_answer: false,
            },
            Meaning {
                meaning: "accepted".into(),
                primary: true,
                accepted_answer: true,
            },
        ];
        let kanji_readings = vec![
            KanjiReading { 
                reading: "not_accepted_reading".into(), 
                primary: true, 
                accepted_answer: false, 
                r#type: super::KanjiType::Nanori 
            },
            KanjiReading { 
                reading: "accepted_reading".into(), 
                primary: true, 
                accepted_answer: true, 
                r#type: super::KanjiType::Nanori 
            },
        ];
        get_kanji(meanings, kanji_readings, vec![])
    }

    #[test]
    fn is_correct_answer_standard_accepted_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_accepted_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        //let meaning = kanji.data.meanings.first().unwrap();
        //meaning.
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_not_accepted_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_incorrect_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    const TEST_ARGS: WaniFmtArgs = WaniFmtArgs {
        radical_args: super::WaniTagArgs { 
            open_tag: "[my_rad]",
            close_tag: "[/my_rad]",
        },
        kanji_args: super::WaniTagArgs { 
            open_tag: "[my_kanji]",
            close_tag: "[/my_kanji]",
        },
        vocab_args: super::WaniTagArgs { 
            open_tag: "[my_vocab]",
            close_tag: "[/my_vocab]",
        },
        meaning_args: super::WaniTagArgs { 
            open_tag: "[my_meaning]",
            close_tag: "[/my_meaning]",
        },
        reading_args: super::WaniTagArgs { 
            open_tag: "[my_reading]",
            close_tag: "[/my_reading]",
        },
        ja_args: super::WaniTagArgs { 
            open_tag: "[my_ja]",
            close_tag: "[/my_ja]",
        },
    };

    #[test]
    fn format_wani_text_no_tags_isnt_changed() {
        let text = "hey there buddy, what is up!!<><hello></hello> swag\n未来";
        let formatted = format_wani_text(text, &TEST_ARGS);
        assert_eq!(text, &formatted);
    }

    #[test]
    fn format_wani_text_tags_are_changed() {
        let text = "this is a <radical>radical</radical>. This is a <kanji>kanji</kanji>.";
        let expected = "this is a [my_rad]radical[/my_rad]. This is a [my_kanji]kanji[/my_kanji].";
        let formatted = format_wani_text(text, &TEST_ARGS);
        assert_eq!(expected, &formatted);

        let text = "this is a <vocabulary>vocab</vocabulary>. This is a <meaning>meaning</meaning>.";
        let expected = "this is a [my_vocab]vocab[/my_vocab]. This is a [my_meaning]meaning[/my_meaning].";
        let formatted = format_wani_text(text, &TEST_ARGS);
        assert_eq!(expected, &formatted);

        let text = "this is a <reading>もうたべた</reading>. This is a <ja>漢字</ja>.";
        let expected = "this is a [my_reading]もうたべた[/my_reading]. This is a [my_ja]漢字[/my_ja].";
        let formatted = format_wani_text(text, &TEST_ARGS);
        assert_eq!(expected, &formatted);
    }

    #[test]
    fn format_wani_empty_args_clears_tags() {
        let text = "this is a <radical>radical</radical>. This is a <kanji>kanji</kanji>.";
        let expected = "this is a radical. This is a kanji.";
        let formatted = format_wani_text(text, &EMPTY_ARGS);
        assert_eq!(expected, &formatted);
    }
}
