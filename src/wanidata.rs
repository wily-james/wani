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
