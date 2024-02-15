use serde::{
    de::{self, Visitor}, Deserialize, Deserializer
};
use reqwest::{
    blocking::Client,
    Error,
};

#[derive(Deserialize, Debug)]
#[serde(tag="object")]
enum WaniData
{
    Collection,
    #[serde(rename="report")]
    Report(Summary),
}

#[derive(Debug, Deserialize)]
struct WaniResp {
    url: String,
    data_updated_at: Option<String>, // TODO - optional for collections if no elements, mandatory
    #[serde(flatten)]
    data: WaniData
}

#[derive(Deserialize, Debug)]
struct Summary {
    data: SummaryData
}

#[derive(Deserialize, Debug)]
struct SummaryData {
    lessons: Vec<Lesson>
}

#[derive(Deserialize, Debug)]
struct Lesson {
    available_at: String,
    subject_ids: Vec<i32>
}


fn main() {
    let client = Client::new();
    let response = client
        .get("https://api.wanikani.com/v2/summary")
        .header("Wanikani-Revision", "20170710")
        .bearer_auth("9610fa33-7c7d-4e34-8c87-6fe17988741a")
        .send()
        .unwrap();

    let wani = response.json::<WaniResp>();
    match wani {
        Err(s) => println!("Error parsing response: {}", s),
        Ok(w) => {
            println!("{:?}", w);
        }
    }
}
