/// Data structures (and some related procedures) to model data going to/from WaniKani api

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use chrono::{
    DateTime,
    Utc,
};
use wana_kana::{IsJapaneseChar, IsJapaneseStr};

/// models a successful response from the WaniKani api
///
/// The structs/enums that are part of the tree rooted with WaniResp are structured the way they
/// are to easily deserialize from the WaniKani API responses
#[derive(Debug, Deserialize)]
pub struct WaniResp {
    #[serde(flatten)]
    pub data: WaniData,
    pub resources_updated: Option<ResourcesUpdated>,
    /*
     * Unused, but part of api
    pub url: String,
    pub data_updated_at: Option<String>,
    */
}

#[derive(Debug, Deserialize)]
pub struct ResourcesUpdated {
    pub assignment: Option<ResourcesUpdatedAssignment>,
}

#[derive(Debug, Deserialize)]
pub struct ResourcesUpdatedAssignment {
    #[serde(flatten)]
    pub data: Assignment,
    /*
     * Unused, but part of api
    pub url: String,
    pub data_updated_at: String,
    */
}

/// rate-limiting info returned by api
#[derive(Debug, Default)]
pub struct RateLimit {
    pub remaining: usize,
    pub reset: u64,
    /*
     * Unused, but part of api
    pub limit: usize,
    */
}

impl RateLimit {
    /// parses RateLimit from api response headers
    pub fn from(headers: &reqwest::header::HeaderMap) -> Option<RateLimit> {
        let remaining = headers.get("RateLimit-Remaining");
        if let None = remaining {
            return None;
        }
        let remaining = remaining.unwrap().to_str();
        if let Err(_) = remaining {
            return None;
        }
        let remaining = remaining.unwrap().parse();
        if let Err(_) = remaining {
            return None;
        }
        let remaining = remaining.unwrap();

        let reset = headers.get("RateLimit-Reset");
        if let None = reset {
            return None;
        } 
        let reset = reset.unwrap().to_str();
        if let Err(_) = reset {
            return None;
        }
        let reset = reset.unwrap().parse();
        if let Err(_) = reset {
            return None;
        }
        let reset = reset.unwrap();

        return Some(RateLimit {
            remaining,
            reset,
        })
    }
}

/// all the possible data types returned by successful api responses
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
    User(User),
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
 
#[derive(Deserialize, Debug, Copy, Clone)]
pub struct Assignment {
    pub id: i32,
    pub data: AssignmentData,
}

#[derive(Deserialize, Debug, Copy, Clone)]
pub struct AssignmentData {
    pub available_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub hidden: bool,
    pub srs_stage: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub subject_id: i32,
    pub subject_type: SubjectType,
    pub unlocked_at: Option<DateTime<Utc>>,
    /*
     * Unused, but part of api
    pub burned_at: Option<DateTime<Utc>>,
    pub passed_at: Option<DateTime<Utc>>,
    pub resurrected_at: Option<DateTime<Utc>>,
    */
}

#[derive(Deserialize, Debug)]
pub struct Review {
    pub data: ReviewData,
    /* 
     * Unused, but part of api
    pub id: i64,
    */
}

#[derive(Deserialize, Debug)]
pub struct ReviewData {
    pub assignment_id: i32,
    /*
     * Unused, but part of the API
    pub created_at: DateTime<Utc>,
    pub ending_srs_stage: u8,
    pub incorrect_meaning_answers: u16,
    pub incorrect_reading_answers: u16,
    pub spaced_repetition_system_id: i32,
    pub starting_srs_stage: u8,
    pub subject_id: i32
    */
}

#[derive(Debug, Serialize)]
pub struct NewReviewRequest {
    pub review: NewReview,
}

#[derive(Debug, Serialize)]
pub struct NewReview {
    #[serde(skip_serializing)]
    pub id: Option<i32>,

    pub assignment_id: i32,
    pub available_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing)]
    pub created_at: DateTime<Utc>,

    pub incorrect_meaning_answers: u16,
    pub incorrect_reading_answers: u16,

    #[serde(skip_serializing)]
    pub status: ReviewStatus,
}

impl Clone for NewReview {
    fn clone(&self) -> Self {
        NewReview {
            id: self.id,
            assignment_id: self.assignment_id,
            available_at: self.available_at,
            created_at: self.created_at,
            incorrect_meaning_answers: self.incorrect_meaning_answers,
            incorrect_reading_answers: self.incorrect_reading_answers,
            status: self.status,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ReviewStatus {
    NotStarted,
    MeaningDone,
    ReadingDone,
    Done
}

impl std::convert::Into<usize> for ReviewStatus {
    fn into(self) -> usize {
        match self {
            ReviewStatus::NotStarted => 0,
            ReviewStatus::MeaningDone => 1,
            ReviewStatus::ReadingDone => 2,
            ReviewStatus::Done => 3,
        }
    }
}

impl std::convert::From<usize> for ReviewStatus {
    fn from(value: usize) -> Self {
        match value {
            0 => ReviewStatus::NotStarted,
            1 => ReviewStatus::MeaningDone,
            2 => ReviewStatus::ReadingDone,
            3 => ReviewStatus::Done,
            _ => panic!(),
        }
    }
}

#[derive(Deserialize, Debug, Copy, Clone)]
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
    pub next_url: Option<String>,
    /*
    pub per_page: i32,
    pub previous_url: Option<String>,
    */
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User
{
    pub data: UserData
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserData
{
    pub id: String,
    pub level: i32,
    pub subscription: Subscription,
}

impl UserData {
    pub fn is_restricted(&self) -> bool {
        self.subscription.max_level_granted < 60
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Subscription {
    pub max_level_granted: i32,
    pub period_ends_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Debug)]
pub struct Radical {
    // Resource Common
    pub id: i32,

    pub data: RadicalData,
}

impl Radical {
    pub fn primary_meanings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.meanings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }
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
    pub url: String,
    pub content_type: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Kanji {
    // Resource Common
    pub id: i32,

    pub data: KanjiData,
}

impl Kanji {
    pub fn primary_meanings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.meanings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }

    pub fn primary_readings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.readings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.reading)
    }

    pub fn alt_meanings<'a>(&'a self) -> impl Iterator<Item = &'a String> {
        self.data.meanings.iter()
            .filter(|m| !m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }

    pub fn alt_readings<'a>(&'a self) -> impl Iterator<Item = &'a String> {
        self.data.readings.iter()
            .filter(|m| !m.primary && m.accepted_answer)
            .map(|m| &m.reading)
    }
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

impl Answer for KanjiReading {
    fn answer<'a>(&'a self) -> (&'a str, bool) {
        (&self.reading, self.accepted_answer)
    }
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

impl Vocab {
    pub fn primary_meanings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.meanings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }

    pub fn primary_readings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.readings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.reading)
    }

    pub fn alt_meanings<'a>(&'a self) -> impl Iterator<Item = &'a String> {
        self.data.meanings.iter()
            .filter(|m| !m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }

    pub fn alt_readings<'a>(&'a self) -> impl Iterator<Item = &'a String> {
        self.data.readings.iter()
            .filter(|m| !m.primary && m.accepted_answer)
            .map(|m| &m.reading)
    }
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

impl Clone for PronunciationAudio {
    fn clone(&self) -> Self {
        PronunciationAudio {
            url: self.url.clone(),
            content_type: self.content_type.clone(),
            metadata: self.metadata.clone(),
        }
    }
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

impl Clone for PronunciationMetadata {
    fn clone(&self) -> Self {
        PronunciationMetadata {
            gender: self.gender.clone(),
            source_id: self.source_id,
            pronunciation: self.pronunciation.clone(),
            voice_actor_id: self.voice_actor_id,
            voice_actor_name: self.voice_actor_name.clone(),
            voice_description: self.voice_description.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VocabReading {
    pub accepted_answer: bool,
    pub primary: bool,
    pub reading: String,
}

impl Answer for VocabReading {
    fn answer<'a>(&'a self) -> (&'a str, bool) {
        (&self.reading, self.accepted_answer)
    }
}

#[derive(Deserialize, Debug)]
pub struct KanaVocab {
    // Resource Common
    pub id: i32,

    pub data: KanaVocabData
}

impl KanaVocab {
    pub fn primary_meanings<'a>(&'a self) -> impl Iterator<Item=&'a String> {
        self.data.meanings.iter()
            .filter(|m| m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }

    pub fn alt_meanings<'a>(&'a self) -> impl Iterator<Item = &'a String> {
        self.data.meanings.iter()
            .filter(|m| !m.primary && m.accepted_answer)
            .map(|m| &m.meaning)
    }
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

trait Answer {
    /// returns: (answer_text, is_accepted_answer)
    fn answer<'a>(&'a self) -> (&'a str, bool);
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meaning {
    pub meaning: String,
    pub primary: bool,
    pub accepted_answer: bool,
}

impl Answer for Meaning {
    fn answer<'a>(&'a self) -> (&'a str, bool) {
        (&self.meaning, self.accepted_answer)
    }
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

impl Answer for AuxMeaning {
    fn answer<'a>(&'a self) -> (&'a str, bool) {
        match self.r#type {
            AuxMeaningType::Whitelist => (&self.meaning, true),
            AuxMeaningType::Blacklist => (&self.meaning, false),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuxMeaningType
{
    #[serde(rename="whitelist")]
    Whitelist,
    #[serde(rename="blacklist")]
    Blacklist
}

/// evaluates a flashcard guess
pub fn is_correct_answer(subject: &Subject, guess: &str, is_meaning: bool, kana_input: &str) -> AnswerResult {
    let is_meaning = is_meaning || match subject {
        Subject::Kanji(_) => false,
        Subject::Vocab(_) => false,
        
        // No readings, so is_meaning should always be true
        Subject::Radical(_) => true,
        Subject::KanaVocab(_) => true,
    };

    if is_meaning {
        return match subject {
            Subject::Radical(r) => {
                is_correct(&r.data.meanings, &Vec::<Meaning>::new(), &r.data.aux_meanings, guess, kana_input, is_meaning)
           },
            Subject::KanaVocab(kv) => {
                is_correct(&kv.data.meanings, &Vec::<Meaning>::new(), &kv.data.aux_meanings, guess, kana_input, true)
            },
            Subject::Kanji(k) => {
                is_correct(&k.data.meanings, &k.data.readings, &k.data.aux_meanings, guess, kana_input, true)
            },
            Subject::Vocab(v) => {
                is_correct(&v.data.meanings, &v.data.readings, &v.data.aux_meanings, guess, kana_input, true)
            },
        };
    }

    let empty_vec = Vec::<Meaning>::new();
    return match subject {
        Subject::Radical(_) => panic!("No readings for radical. should be unreachable."),
        Subject::KanaVocab(_) => panic!("No readings for kana vocab. should be unreachable."),
        Subject::Kanji(k) => is_correct(&k.data.readings, &empty_vec, &empty_vec, guess, "", false),
        Subject::Vocab(v) => is_correct(&v.data.readings, &empty_vec, &empty_vec, guess, "", false),
    };
}

fn is_correct<T, U, V>(meanings: &Vec<T>, readings: &Vec<U>, aux_meanings: &Vec<V>, guess: &str, kana_input: &str, allow_fuzzy: bool) -> AnswerResult
where T: Answer, U: Answer, V: Answer {
    let mut expect_numeric = false;
    let mut best = AnswerResult::Incorrect;
    
    for m in meanings {
        // Warning: this block is copy/pasted
        let (meaning, is_accepted_answer) = m.answer();
        if guess == meaning.trim().to_lowercase() {
            if is_accepted_answer {
                return AnswerResult::Correct;
            }

            best = AnswerResult::MatchesNonAcceptedAnswer;
        }

        if is_accepted_answer && meaning.chars().any(|c| c.is_numeric()) {
            expect_numeric = true;
        }
    }

    for m in aux_meanings {
        // Warning: this block is copy/pasted
        let (meaning, is_accepted_answer) = m.answer();
        if guess == meaning.trim().to_lowercase() {
            if is_accepted_answer {
                return AnswerResult::Correct;
            }

            best = AnswerResult::MatchesNonAcceptedAnswer;
        }

        if is_accepted_answer && meaning.chars().any(|c| c.is_numeric()) {
            expect_numeric = true;
        }
    }

    if meanings.len() > 0 {
        if let AnswerResult::Correct = is_correct::<U, T, V>(readings, &vec![], &vec![], kana_input, "", false) {
            return AnswerResult::KanaWhenMeaning;
        }
    }

    if let AnswerResult::Incorrect = best {
        if guess.chars().any(|c| {
            if expect_numeric {
                return !c.is_alphanumeric() && !c.is_kana() && c != ' ';
            }

            !c.is_alphabetic() && !c.is_kana() && c != ' '
        }) {
            return AnswerResult::BadFormatting;
        }

        if guess.is_mixed() {
            return AnswerResult::BadFormatting;
        }

        if !allow_fuzzy {
            return best;
        }

        for m in meanings {
            let (meaning, is_accepted_answer) = m.answer();
            if fuzzy_accept(guess, &meaning.trim().to_lowercase()) {
                if is_accepted_answer {
                    return AnswerResult::FuzzyCorrect;
                }
                else {
                    best = AnswerResult::MatchesNonAcceptedAnswer;
                }
            }
        }

        for m in aux_meanings {
            let (meaning, is_accepted_answer) = m.answer();
            if fuzzy_accept(guess, &meaning.trim().to_lowercase()) {
                if is_accepted_answer {
                    return AnswerResult::FuzzyCorrect;
                }
                else {
                    best = AnswerResult::MatchesNonAcceptedAnswer;
                }
            }
        }
    }

    return best;
}

fn fuzzy_accept(guess: &str, answer: &str) -> bool {
    match answer.len() {
        0 | 1 | 2 | 3  => {
            false
        },
        4 | 5 => {
            edit_distance(guess, answer) <= 1
        },
        6 | 7 => {
            edit_distance(guess, answer) <= 2
        },
        n => {
            edit_distance(guess, answer) <= (n / 7 + 2)
        }
    }
}

fn edit_distance(s: &str, t: &str) -> usize {
    let s = s.chars().collect_vec();
    let t = t.chars().collect_vec();

    let n = s.len();
    let m = t.len();

    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev = Vec::with_capacity(m + 1);
    let mut curr = Vec::with_capacity(n + 1);

    for i in 0..m+1 {
        prev.push(i);
    }

    for i in 1..n+1 {
        curr.push(i);
        for j in 1..m + 1 {
            if s[i-1] == t[j-1] {
                curr.push(prev[j-1]);
            }
            else {
                let min = std::cmp::min(1 + prev[j], 1 + curr[j - 1]);
                curr.push(std::cmp::min(min, 1 + prev[j - 1]));
            }
        }
        prev = curr;
        curr = Vec::with_capacity(n + 1);
    }

    prev[m]
}

/// options to format display strings from wanikani servers
#[derive(Default)]
pub struct WaniFmtArgs {
    pub radical_args: WaniTagArgs,
    pub kanji_args: WaniTagArgs,
    pub vocab_args: WaniTagArgs,
    pub meaning_args: WaniTagArgs,
    pub reading_args: WaniTagArgs,
    pub ja_args: WaniTagArgs,
}

/// specifies an open and close tag to replace custom wanikani tags with
#[derive(Default)]
pub struct WaniTagArgs {
    pub open_tag: String,
    pub close_tag: String,
}

/// replaces custom tags sent in display strings from wanikani servers
pub fn format_wani_text(s: &str, args: &WaniFmtArgs) -> String {
    let s = s.replace("<radical>", &args.radical_args.open_tag);
    let s = s.replace("</radical>", &args.radical_args.close_tag);
    let s = s.replace("<kanji>", &args.kanji_args.open_tag);
    let s = s.replace("</kanji>", &args.kanji_args.close_tag);
    let s = s.replace("<vocabulary>", &args.vocab_args.open_tag);
    let s = s.replace("</vocabulary>", &args.vocab_args.close_tag);
    let s = s.replace("<reading>", &args.reading_args.open_tag);
    let s = s.replace("</reading>", &args.reading_args.close_tag);
    let s = s.replace("<ja>", &args.ja_args.open_tag);
    let s = s.replace("</ja>", &args.ja_args.close_tag);
    let s = s.replace("<meaning>", &args.meaning_args.open_tag);
    s.replace("</meaning>", &args.meaning_args.close_tag)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use crate::wanidata::{edit_distance, AnswerResult};
    use super::{format_wani_text, is_correct_answer, AuxMeaning, AuxMeaningType, KanaVocab, KanaVocabData, Kanji, KanjiData, KanjiReading, Meaning, Radical, RadicalData, Subject, Vocab, VocabData, VocabReading, WaniFmtArgs};

    // #region is_correct_answer Kanji

    #[test]
    fn is_correct_answer_accepted_kanji_meaning_edit_distance() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepterd", is_meaning, "");

        assert!(matches!(result, AnswerResult::FuzzyCorrect));
    }

    #[test]
    fn is_correct_answer_low_edit_dist_but_matches_non_accepted() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepted1", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_reading_doesnt_check_edit_dist() {
        let is_meaning = false;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "はがねん", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_high_edit_dist() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "acceptedlmno", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_short_answer_strict() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "b", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_shortish_answer_accepts_close() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accr", is_meaning, "");

        assert!(matches!(result, AnswerResult::FuzzyCorrect));
    }

    #[test]
    fn is_correct_answer_shortish_answer_rejects_far() {
        let is_meaning = true;
        let kanji = get_edit_dist_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accerp", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_kanji_on_whitelist() {
        let is_meaning = true;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "aux_whitelist";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_kanji_on_whitelist_fuzzy() {
        let is_meaning = true;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "whitelisty";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::FuzzyCorrect));
    }

    #[test]
    fn is_correct_answer_kanji_on_blacklist() {
        let is_meaning = true;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "aux_blacklist";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_kanji_on_blacklist_fuzzy() {
        let is_meaning = true;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "blacklisty";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_kanji_matches_no_aux() {
        let is_meaning = true;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "auxnone";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_kanji_matches_whitelist_but_is_reading_special_char() {
        let is_meaning = false;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "aux_whitelist";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::BadFormatting));
    }

    #[test]
    fn is_correct_answer_kanji_matches_whitelist_but_is_reading() {
        let is_meaning = false;
        let kanji = get_aux_meaning_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "whitelist";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_illegal_chars() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let subj = Subject::Kanji(kanji);
        for guess in "0123456789!@#$%^&*()-_=+`~[[]]\\;:'\",<.>/?".chars() {
            let guess = String::from(guess);
            let result = is_correct_answer(&subj, &guess, is_meaning, "");

            assert!(matches!(result, AnswerResult::BadFormatting));
        }
    }

    #[test]
    fn is_correct_answer_mixed_kana() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "おn";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::BadFormatting));
    }

    #[test]
    fn is_correct_answer_mixed_kana_reading() {
        let is_meaning = false;
        let kanji = get_standard_kanji();
        let subj = Subject::Kanji(kanji);
        let guess = "おn";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::BadFormatting));
    }

    #[test]
    fn is_correct_answer_expects_number_allows_numbers() {
        let is_meaning = true;
        let mut kanji = get_standard_kanji();
        kanji.data.meanings.push(Meaning { 
            meaning: "42".into(), 
            primary: false, 
            accepted_answer: true 
        });

        let subj = Subject::Kanji(kanji);
        let guess = "43";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_expects_number_rejects_spaces() {
        let is_meaning = true;
        let mut kanji = get_standard_kanji();
        kanji.data.meanings.push(Meaning { 
            meaning: "42".into(), 
            primary: false, 
            accepted_answer: true 
        });

        let subj = Subject::Kanji(kanji);
        let guess = "hello there";
        let result = is_correct_answer(&subj, &guess, is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_accepted_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_accepted_kanji_reading() {
        let is_meaning = false;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "はがねの", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_gave_kanji_reading_when_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "blah", is_meaning, "はがねの");

        assert!(matches!(result, AnswerResult::KanaWhenMeaning));
    }

    #[test]
    fn is_correct_answer_not_accepted_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_not_accepted_kanji_reading() {
        let is_meaning = false;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "not_はがねの", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_kanji_meaning() {
        let is_meaning = true;
        let mut kanji = get_standard_kanji();
        kanji.data.meanings.push(Meaning { 
            meaning: " accepted1\n".into(), 
            primary: false, 
            accepted_answer: true 
        });
        let result = is_correct_answer(&Subject::Kanji(kanji), "accepted1", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_kanji_reading() {
        let is_meaning = false;
        let mut kanji = get_standard_kanji();
        kanji.data.readings.push(KanjiReading { 
            reading: " はがねのの\n".into(), 
            primary: false, 
            accepted_answer: true,
            r#type: crate::wanidata::KanjiType::Nanori,
        });
        let result = is_correct_answer(&Subject::Kanji(kanji), "はがねのの", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_incorrect_kanji_meaning() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_incorrect_kanji_meaning_with_spaces() {
        let is_meaning = true;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "foo bar", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_incorrect_kanji_reading() {
        let is_meaning = false;
        let kanji = get_standard_kanji();
        let result = is_correct_answer(&Subject::Kanji(kanji), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    // #endregion is_correct_answer Kanji
    
    // #region is_correct_answer Vocab

    #[test]
    fn is_correct_answer_accepted_vocab_meaning() {
        let is_meaning = true;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_gave_reading_when_meaning() {
        let is_meaning = true;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "blah", is_meaning, "はがねの");

        assert!(matches!(result, AnswerResult::KanaWhenMeaning));
    }

    #[test]
    fn is_correct_answer_accepted_vocab_reading() {
        let is_meaning = false;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "はがねの", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_not_accepted_vocab_meaning() {
        let is_meaning = true;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_not_accepted_vocab_reading() {
        let is_meaning = false;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "not_はがねの", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_vocab_meaning() {
        let is_meaning = true;
        let mut vocab = get_standard_vocab();
        vocab.data.meanings.push(Meaning { 
            meaning: " accepted1\n".into(), 
            primary: false, 
            accepted_answer: true 
        });
        let result = is_correct_answer(&Subject::Vocab(vocab), "accepted1", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_vocab_reading() {
        let is_meaning = false;
        let mut vocab = get_standard_vocab();
        vocab.data.readings.push(VocabReading { 
            reading: " はがねのの\n".into(), 
            primary: false, 
            accepted_answer: true,
        });
        let result = is_correct_answer(&Subject::Vocab(vocab), "はがねのの", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_incorrect_vocab_meaning() {
        let is_meaning = true;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_incorrect_vocab_reading() {
        let is_meaning = false;
        let vocab = get_standard_vocab();
        let result = is_correct_answer(&Subject::Vocab(vocab), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    // #endregion is_correct_answer Vocab

    // #region is_correct_answer KanaVocab
    
    #[test]
    fn is_correct_answer_accepted_kv() {
        let is_meaning = true;
        let kv = get_standard_kana_vocab();
        let result = is_correct_answer(&Subject::KanaVocab(kv), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }
    
    #[test]
    fn is_correct_answer_accepted_kv_ignores_is_meaning() {
        let is_meaning = false;
        let kv = get_standard_kana_vocab();
        let result = is_correct_answer(&Subject::KanaVocab(kv), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_not_accepted_kv() {
        let is_meaning = true;
        let kv = get_standard_kana_vocab();
        let result = is_correct_answer(&Subject::KanaVocab(kv), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_kv() {
        let is_meaning = true;
        let mut kv = get_standard_kana_vocab();
        kv.data.meanings.push(Meaning { 
            meaning: " accepted1\n".into(), 
            primary: false, 
            accepted_answer: true 
        });
        let result = is_correct_answer(&Subject::KanaVocab(kv), "accepted1", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_incorrect_kv() {
        let is_meaning = true;
        let kv = get_standard_kana_vocab();
        let result = is_correct_answer(&Subject::KanaVocab(kv), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    //
    // #endregion is_correct_answer KanaVocab

    // #region is_correct_answer Radical
    
    #[test]
    fn is_correct_answer_accepted_radical() {
        let is_meaning = true;
        let radical = get_standard_radical();
        let result = is_correct_answer(&Subject::Radical(radical), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }
    
    #[test]
    fn is_correct_answer_accepted_radical_ignores_is_meaning() {
        let is_meaning = false;
        let radical = get_standard_radical();
        let result = is_correct_answer(&Subject::Radical(radical), "accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_not_accepted_radical() {
        let is_meaning = true;
        let radical = get_standard_radical();
        let result = is_correct_answer(&Subject::Radical(radical), "not_accepted", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_accepted_with_whitespace_radical() {
        let is_meaning = true;
        let mut radical = get_standard_radical();
        radical.data.meanings.push(Meaning { 
            meaning: " accepted1\n".into(), 
            primary: false, 
            accepted_answer: true 
        });
        let result = is_correct_answer(&Subject::Radical(radical), "accepted1", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_incorrect_radical() {
        let is_meaning = true;
        let radical = get_standard_radical();
        let result = is_correct_answer(&Subject::Radical(radical), "foo", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    #[test]
    fn is_correct_answer_aux_meaning_blacklist() {
        let is_meaning = true;
        let radical = get_radical_aux_meanings();
        let result = is_correct_answer(&Subject::Radical(radical), "aux_blacklist", is_meaning, "");

        assert!(matches!(result, AnswerResult::MatchesNonAcceptedAnswer));
    }

    #[test]
    fn is_correct_answer_aux_meaning_whitelist() {
        let is_meaning = true;
        let radical = get_radical_aux_meanings();
        let result = is_correct_answer(&Subject::Radical(radical), "aux_whitelist", is_meaning, "");

        assert!(matches!(result, AnswerResult::Correct));
    }

    #[test]
    fn is_correct_answer_aux_meaning_guess_matches_none() {
        let is_meaning = true;
        let radical = get_radical_aux_meanings();
        let result = is_correct_answer(&Subject::Radical(radical), "auxnone", is_meaning, "");

        assert!(matches!(result, AnswerResult::Incorrect));
    }

    // #endregion is_correct_answer Radical

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

    fn get_standard_radical() -> Radical {
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

        get_radical(meanings, vec![])
    }

    fn get_radical_aux_meanings() -> Radical {
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

        let aux_meanings = vec![
            AuxMeaning { 
                r#type: AuxMeaningType::Blacklist, 
                meaning: "aux_blacklist".into(), 
            },
            AuxMeaning { 
                r#type: AuxMeaningType::Whitelist, 
                meaning: "aux_whitelist".into(), 
            },
        ];

        get_radical(meanings, aux_meanings)
    }

    fn get_radical(meanings: Vec<Meaning>, aux_meanings: Vec<AuxMeaning>) -> Radical {
        Radical {
            id: 1,
            data: RadicalData {
                aux_meanings,
                meanings,
                created_at: Utc::now(),
                document_url: "".into(),
                hidden_at: None,
                lesson_position: 1,
                level: 1,
                meaning_mnemonic: "".into(),
                slug: "".into(),
                spaced_repetition_system_id: 1,
                amalgamation_subject_ids: vec![],
                characters: None,
                character_images: vec![],
            }
        }
    }

    fn get_standard_kana_vocab() -> KanaVocab {
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
        get_kana_vocab(meanings, vec![])
    }

    fn get_kana_vocab(meanings: Vec<Meaning>, aux_meanings: Vec<AuxMeaning>) -> KanaVocab {
        KanaVocab {
            id: 1,
            data: KanaVocabData {
                aux_meanings,
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
                context_sentences: vec![],
                parts_of_speech: vec![],
                pronunciation_audios: vec![],
            }
        }
    }

    fn get_standard_vocab() -> super::Vocab {
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
        let vocab_readings = vec![
            VocabReading { 
                reading: "not_はがねの".into(), 
                primary: true, 
                accepted_answer: false, 
            },
            VocabReading { 
                reading: "はがねの".into(), 
                primary: true, 
                accepted_answer: true, 
            },
        ];
        get_vocab(meanings, vocab_readings, vec![])
    }

    fn get_vocab(meanings: Vec<Meaning>, readings: Vec<VocabReading>, aux_meanings: Vec<AuxMeaning>) -> Vocab {
        Vocab {
            id: 1,
            data: VocabData {
                readings,
                meanings,
                aux_meanings,
                created_at: Utc::now(),
                document_url: "".into(),
                hidden_at: None,
                lesson_position: 1,
                level: 1,
                meaning_mnemonic: "".into(),
                slug: "".into(),
                spaced_repetition_system_id: 1,
                characters: "".into(),
                component_subject_ids: vec![],
                context_sentences: vec![],
                parts_of_speech: vec![],
                pronunciation_audios: vec![],
                reading_mnemonic: "".into(),
            }
        }
    }

    fn get_aux_meaning_kanji() -> Kanji {
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
        let aux_meanings = vec![
            AuxMeaning { 
                r#type: AuxMeaningType::Blacklist, 
                meaning: "aux_blacklist".into(), 
            },
            AuxMeaning { 
                r#type: AuxMeaningType::Blacklist, 
                meaning: "blacklist".into(), 
            },
            AuxMeaning { 
                r#type: AuxMeaningType::Whitelist, 
                meaning: "aux_whitelist".into(), 
            },
            AuxMeaning { 
                r#type: AuxMeaningType::Whitelist, 
                meaning: "whitelist".into(), 
            },
        ];
        let kanji_readings = vec![
            KanjiReading { 
                reading: "not_はがねの".into(), 
                primary: true, 
                accepted_answer: false, 
                r#type: super::KanjiType::Nanori 
            },
            KanjiReading { 
                reading: "はがねの".into(), 
                primary: true, 
                accepted_answer: true, 
                r#type: super::KanjiType::Nanori 
            },
        ];
        get_kanji(meanings, kanji_readings, aux_meanings)
    }

    fn get_edit_dist_kanji() -> Kanji {
        let meanings = vec![
            Meaning {
                meaning: "accepted1".into(),
                primary: false,
                accepted_answer: false,
            },
            Meaning {
                meaning: "accepted".into(),
                primary: true,
                accepted_answer: true,
            },
            Meaning {
                meaning: "a".into(),
                primary: true,
                accepted_answer: true,
            },
            Meaning {
                meaning: "acce".into(),
                primary: true,
                accepted_answer: true,
            },
        ];
        let kanji_readings = vec![
            KanjiReading { 
                reading: "not_はがねの".into(), 
                primary: true, 
                accepted_answer: false, 
                r#type: super::KanjiType::Nanori 
            },
            KanjiReading { 
                reading: "はがねの".into(), 
                primary: true, 
                accepted_answer: true, 
                r#type: super::KanjiType::Nanori 
            },
        ];
        get_kanji(meanings, kanji_readings, vec![])
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
                reading: "not_はがねの".into(), 
                primary: true, 
                accepted_answer: false, 
                r#type: super::KanjiType::Nanori 
            },
            KanjiReading { 
                reading: "はがねの".into(), 
                primary: true, 
                accepted_answer: true, 
                r#type: super::KanjiType::Nanori 
            },
        ];
        get_kanji(meanings, kanji_readings, vec![])
    }


    fn test_args() -> WaniFmtArgs { 
        WaniFmtArgs {
            radical_args: super::WaniTagArgs { 
                open_tag: "[my_rad]".to_owned(),
                close_tag: "[/my_rad]".to_owned(),
            },
            kanji_args: super::WaniTagArgs { 
                open_tag: "[my_kanji]".to_owned(),
                close_tag: "[/my_kanji]".to_owned(),
            },
            vocab_args: super::WaniTagArgs { 
                open_tag: "[my_vocab]".to_owned(),
                close_tag: "[/my_vocab]".to_owned(),
            },
            meaning_args: super::WaniTagArgs { 
                open_tag: "[my_meaning]".to_owned(),
                close_tag: "[/my_meaning]".to_owned(),
            },
            reading_args: super::WaniTagArgs { 
                open_tag: "[my_reading]".to_owned(),
                close_tag: "[/my_reading]".to_owned(),
            },
            ja_args: super::WaniTagArgs { 
                open_tag: "[my_ja]".to_owned(),
                close_tag: "[/my_ja]".to_owned(),
            },
        } 
    }

    #[test]
    fn format_wani_text_no_tags_isnt_changed() {
        let text = "hey there buddy, what is up!!<><hello></hello> swag\n未来";
        let formatted = format_wani_text(text, &test_args());
        assert_eq!(text, &formatted);
    }

    #[test]
    fn format_wani_text_tags_are_changed() {
        let text = "this is a <radical>radical</radical>. This is a <kanji>kanji</kanji>.";
        let expected = "this is a [my_rad]radical[/my_rad]. This is a [my_kanji]kanji[/my_kanji].";
        let formatted = format_wani_text(text, &test_args());
        assert_eq!(expected, &formatted);

        let text = "this is a <vocabulary>vocab</vocabulary>. This is a <meaning>meaning</meaning>.";
        let expected = "this is a [my_vocab]vocab[/my_vocab]. This is a [my_meaning]meaning[/my_meaning].";
        let formatted = format_wani_text(text, &test_args());
        assert_eq!(expected, &formatted);

        let text = "this is a <reading>もうたべた</reading>. This is a <ja>漢字</ja>.";
        let expected = "this is a [my_reading]もうたべた[/my_reading]. This is a [my_ja]漢字[/my_ja].";
        let formatted = format_wani_text(text, &test_args());
        assert_eq!(expected, &formatted);
    }

    #[test]
    fn format_wani_empty_args_clears_tags() {
        let text = "this is a <radical>radical</radical>. This is a <kanji>kanji</kanji>.";
        let expected = "this is a radical. This is a kanji.";
        let formatted = format_wani_text(text, &WaniFmtArgs::default());
        assert_eq!(expected, &formatted);
    }

    // #region test edit_distance

    #[test]
    fn edit_distance_empty_equal_to_other() {
        let s = "";
        let t = "foo";
        let expected = 3;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_empty_equal_to_other_reversed() {
        let s = "foobar";
        let t = "";
        let expected = 6;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_1() {
        let s = "foo";
        let t = "foof";
        let expected = 1;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_2() {
        let s = "";
        let t = "1";
        let expected = 1;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_3() {
        let s = "2";
        let t = "1";
        let expected = 1;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_4() {
        let s = "foobar";
        let t = "foo";
        let expected = 3;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_5() {
        let s = "foobar";
        let t = "boo";
        let expected = 4;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_6() {
        let s = "";
        let t = "";
        let expected = 0;
        assert_eq!(expected, edit_distance(s, t));
    }

    #[test]
    fn edit_distance_7() {
        let s = "おはよう";
        let t = "おはのう";
        let expected = 1;
        assert_eq!(expected, edit_distance(s, t));
    }

    // #endregion test edit_distance
}
