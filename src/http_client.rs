use failure::format_err;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT_RANGES, AUTHORIZATION, RANGE, USER_AGENT,
};
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::{from_str, Value};

use crate::model::Auth;
use crate::result::Result;

const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.21 (KHTML, like Gecko) Mwendo/1.1.5 Safari/537.21";
const CHUNK: u64 = 2 * 1024 * 1024;

pub struct UdemyHttpClient {
    client: Client,
}

pub trait HttpClient {
    fn get_as_text(&self, url: &str, auth: &Auth) -> Result<String>;
    fn get_as_json(&self, url: &str, auth: &Auth) -> Result<Value> {
        self.get_as_text(url, auth).map(|text| {
            from_str(text.as_str())
                .map_err(|e| format_err!("Error parsing json from url <{}>: {:?}", url, e))
        })?
    }
    fn get_as_data(&self, url: &str, f: &mut dyn FnMut(u64)) -> Result<Vec<u8>>;
    fn get_content_length(&self, url: &str) -> Result<u64>;
    fn post_json(&self, url: &str, json: &Value, auth: &Auth) -> Result<()>;
}

impl HttpClient for UdemyHttpClient {
    fn get_as_text(&self, url: &str, auth: &Auth) -> Result<String> {
        let mut resp = self
            .client
            .get(url)
            .headers(self.construct_headers(auth))
            .send()?;
        if resp.status().is_success() {
            Ok(resp.text()?)
        } else {
            Err(format_err!(
                "Error while getting from url <{}>: <{}>",
                url,
                resp.status()
            ))
        }
    }

    fn get_content_length(&self, url: &str) -> Result<u64> {
        let resp = self
            .client
            .head(url)
            // .headers(self.construct_headers())
            .send()?;
        if resp.status().is_success() {
            Ok(resp
                .content_length()
                .ok_or_else(|| format_err!("Error getting length of url <{}>", url))?)
        } else {
            Err(format_err!(
                "Error while trying to access url <{}> - <{}>",
                url,
                resp.status()
            ))
        }
    }

    fn get_as_data(&self, url: &str, f: &mut dyn FnMut(u64)) -> Result<Vec<u8>> {
        let http_range = self.has_http_range(url)?;
        if http_range {
            let total = self.get_content_length(url)?;
            let mut offset = 0_u64;
            let mut buf = Vec::with_capacity(total as usize);

            loop {
                let mut temp_buf = Vec::with_capacity(CHUNK as usize);
                let mut resp = self
                    .client
                    .get(url)
                    .header(RANGE, format!("bytes={}-{}", offset, offset + CHUNK - 1))
                    .send()?;
                match resp.status() {
                    StatusCode::PARTIAL_CONTENT => {
                        resp.copy_to(&mut temp_buf)?;
                        buf.append(&mut temp_buf);
                        (*f)(offset + CHUNK);

                        offset += CHUNK;
                        if offset > total {
                            break;
                        }
                    }
                    StatusCode::OK => {
                        resp.copy_to(&mut buf)?;
                        break;
                    }
                    _ => {
                        return Err(format_err!("Error received {:?}", resp.status()));
                    }
                }
            }
            Ok(buf)
        } else {
            let mut resp = self.client.get(url).send()?;
            if resp.status().is_success() {
                let mut buf: Vec<u8> = vec![];
                let size = resp.copy_to(&mut buf)?;
                (*f)(size);
                Ok(buf)
            } else {
                Err(format_err!("Error while getting from url <{}>", url))
            }
        }
    }

    fn post_json(&self, url: &str, json: &Value, auth: &Auth) -> Result<()> {
        self.client
            .post(url)
            .headers(self.construct_headers(auth))
            .json(json)
            .send()?;
        Ok(())
    }
}

impl UdemyHttpClient {
    pub fn new() -> UdemyHttpClient {
        let client = Client::new();
        UdemyHttpClient { client }
    }

    fn has_http_range(&self, url: &str) -> Result<bool> {
        self.client
            .head(url)
            .send()
            .map(|res| res.headers().contains_key(ACCEPT_RANGES))
            .map_err(|_e| format_err!("Could not check http range"))
    }

    fn construct_headers(&self, auth: &Auth) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", auth.access_token.as_ref().unwrap());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(bearer.as_str()).unwrap(),
        );
        headers.insert(
            HeaderName::from_lowercase(b"x-udemy-authorization").unwrap(),
            HeaderValue::from_str(bearer.as_str()).unwrap(),
        );
        headers.insert(USER_AGENT, HeaderValue::from_str(DEFAULT_UA).unwrap());
        headers
    }
}
