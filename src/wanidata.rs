use serde::Deserialize;
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
    #[serde(rename="assignment")]

    // Resources:
    Assignment,
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
    Review,
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

#[derive(Deserialize, Debug)]
pub struct Collection {
    pub data: Vec<WaniData>,
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
    //character_images
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

#[derive(Deserialize, Debug)]
pub struct KanjiReading {
    pub reading: String,
    pub primary: bool,
    pub accepted_answer: bool,
    pub r#type: KanjiType,
}

#[derive(Deserialize, Debug)]
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

#[derive(Deserialize, Debug)]
pub struct ContextSentence {
    pub en: String,
    pub ja: String,
}

#[derive(Deserialize, Debug)]
pub struct PronunciationAudio {
    pub url: String,
    pub content_type: String,
    pub metadata: PronunciationMetadata
}

#[derive(Deserialize, Debug)]
pub struct PronunciationMetadata
{
    pub gender: String,
    pub source_id: i32,
    pub pronunciation: String,
    pub voice_actor_id: i32,
    pub voice_actor_name: String,
    pub voice_description: String,
}

#[derive(Deserialize, Debug)]
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

#[derive(Deserialize, Debug)]
pub struct Meaning {
    pub meaning: String,
    pub primary: bool,
    pub accepted_answer: bool,
}

#[derive(Deserialize, Debug)]
pub struct AuxMeaning {
    pub r#type: AuxMeaningType,
    pub meaning: String,
}

#[derive(Deserialize, Debug)]
pub enum AuxMeaningType
{
    #[serde(rename="whitelist")]
    Whitelist,
    #[serde(rename="blacklist")]
    Blacklist
}

#[derive(Deserialize, Debug)]
pub enum CacheInfoType
{
    Resources = 0,
}

pub enum CacheInfoSchema
{
    Type,
    Etag,
    LastModified,
}

impl Into<usize> for CacheInfoSchema {
    fn into(self) -> usize {
        match self {
            CacheInfoSchema::Type => 0,
            CacheInfoSchema::Etag => 1,
            CacheInfoSchema::LastModified => 2,
        }
    }
}
