use std::collections::BTreeMap;
use std::fmt;
use ordinals::RuneId;
use serde::{de, Deserialize, Deserializer};
use serde::de::Visitor;

#[derive(Deserialize, Debug)]
pub struct RuneEntry {
    pub spaced_rune: String,
    pub mints: u128,
    pub premine: u128,
    pub divisibility: u16,
    pub number: u128,
}

#[derive(Deserialize, Debug)]
pub struct RuneResponse {
    pub entry: RuneEntry,
    pub parent: Option<String>,
}

fn string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
{
    struct StringOrNumberVisitor;

    impl<'de> Visitor<'de> for StringOrNumberVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or a number")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
        {
            value.parse::<f64>().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(StringOrNumberVisitor)
}

#[derive(Deserialize, Debug)]
pub struct RuneBalance {
    pub rune_name: String,
    #[serde(deserialize_with = "string_or_number")]
    pub balance: f64,
    pub rune_symbol: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AddressResponse {
    pub outputs: Vec<String>,
    pub inscriptions: Vec<String>,
    pub sat_balance: u64,
    pub runes_balances: Vec<RuneBalance>,
}

#[derive(Deserialize, Debug)]
pub struct Output {
    pub address: Option<String>,
    pub indexed: bool,
    pub outpoint: String,
    pub runes: BTreeMap<String, Pile>,
    pub sat_ranges: Option<Vec<(u64, u64)>>,
    pub script_pubkey: String,
    pub spent: bool,
    pub transaction: String,
    pub value: u64,
}

#[derive(Deserialize, Debug)]
pub struct Pile {
    pub amount: u64,
    pub divisibility: u16,
    pub symbol: String,
}


pub struct OrdClient {
    client: reqwest::blocking::Client,
    base_api_url: String,
}

impl OrdClient {
    pub fn new(ord_base_url: String) -> Self {
        let client = reqwest::blocking::Client::new();
        OrdClient {
            client,
            base_api_url: ord_base_url,
        }
    }

    pub fn fetch_data(&self, url: &str) -> String {
        let res = self.client
            .get(url)
            .header("Accept", "application/json")
            .send();

        res.expect("get method should not fail")
            .text()
            .expect("result should be text decodable")
    }


    pub fn fetch_rune_details(&self, rune_id: RuneId) -> RuneResponse {
        let api_response = self.fetch_data(&format!("{}/rune/{}", self.base_api_url, rune_id));
        serde_json::from_str::<RuneResponse>(&api_response).unwrap()
    }

    pub fn fetch_latest_block_height(&self) -> u64 {
        let api_response = self.fetch_data(&format!("{}/blockheight", self.base_api_url));
        serde_json::from_str::<u64>(&api_response).unwrap()
    }

    pub fn get_address(&self, address: &str) -> AddressResponse {
        let api_response = self.fetch_data(&format!("{}/address/{}", self.base_api_url, address));
        serde_json::from_str::<AddressResponse>(&api_response).unwrap()
    }

    pub fn get_outputs_for_address(&self, address: &str) -> Vec<Output> {
        let api_response = self.fetch_data(&format!("{}/outputs/{}", self.base_api_url, address));
        serde_json::from_str::<Vec<Output>>(&api_response).unwrap()
    }
}

pub fn wait_for_block(ord_client: &OrdClient, block: u64) {
    let mut countdown = 12;
    while ord_client.fetch_latest_block_height() < block {
        std::thread::sleep(std::time::Duration::from_secs(1));
        countdown -= 1;
        if countdown == 0 {
            assert!(false, "timed out waiting for ord to process blocks")
        }
    }
}