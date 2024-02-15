use serde::{
    de::{self, Visitor}, Deserialize, Deserializer
};
use reqwest::{
    blocking::Client,
    Error,
};

#[derive(serde::Deserialize, Debug, Default)]
enum WaniData
{
    #[default]
    Collection,
    Summary(Summary),
}

#[derive(Debug, Default)]
struct WaniResp {
    object: String,
    url: String,
    data_updated_at: Option<String>, // TODO - optional for collections if no elements, mandatory
                                     // for resources
    data: WaniData
}

struct WaniRespVisitor {}

impl<'de> Visitor<'de> for WaniRespVisitor {
    type Value = WaniResp;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Could not deserialize WaniResp")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut element = WaniResp { ..Default::default() };
        let mut object: Option<String> = None;
        let mut url: Option<String> = None;
        let mut data_updated_at: Option<String> = None;
        let mut data: Option<serde_json::Value> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "object" => {
                    if let Ok(s) = map.next_value::<String>()
                    {
                        object = Some(s);
                    }
                },
                "url" => {
                    if let Ok(s) = map.next_value::<String>()
                    {
                        url = Some(s);
                    }
                },
                "data_updated_at" => {
                    if let Ok(s) = map.next_value::<String>()
                    {
                        data_updated_at = Some(s);
                    }
                },
                "data" => {
                    if let Ok(m) = map.next_value::<serde_json::Value>()
                    {
                        data = Some(m);
                    }
                }
                _ => (),
            }
        }

        if object.is_none() {
            return Err(de::Error::missing_field("object"));
        }

        if url.is_none() {
            return Err(de::Error::missing_field("url"));
        }

        if data.is_none() {
            return Err(de::Error::missing_field("data"));
        }

        if let Some(obj) = object
        {
            match obj.as_str() {
                "collection" => {},
                "report" => {
                    if let serde_json::Value::Object(m) = data.unwrap()
                    {

                    }
                    else {
                        return Err(de::Error::custom("report data was not a json object."))
                    }
                },
                "assignment" => {},
                "kana_vocabulary" => {},
                "kanji" => {},
                "level_progression" => {},
                "radical" => {},
                "reset" => {},
                "review_statistic" => {},
                "review" => {},
                "spaced_repetition_system" => {},
                "study_material" => {},
                "user" => {},
                "vocabulary" => {},
                "voice_actor" => {},
                _ => ()
            };
        }

        Ok(element)
    }
}

impl<'de> Deserialize<'de> for WaniResp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {
        deserializer.deserialize_map(WaniRespVisitor {})
    }
}

#[derive(serde::Deserialize, Debug, Default)]
struct Summary {
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

    let wani = response.json::<WaniResp>().unwrap();
    println!("{:?}", wani);
}
