use std::time::Duration;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("marketfeed-rs/0.1")
        .build()
        .expect("valid reqwest client")
}
