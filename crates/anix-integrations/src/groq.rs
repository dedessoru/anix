//! Transport policy for explicitly authorized synthetic Groq Research Lead runs.
use anix_domain::{Error, Result};
use reqwest::{blocking::Client, header::HeaderMap};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default, Serialize)]
pub(crate) struct GroqHttpMetrics {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    rate_limit_events: u64,
    retries: u64,
    latency_ms: u128,
    last_rate_limits: Value,
    // The API usage response does not establish account billing charges.
    cloud_cost_usd: Option<f64>,
}

fn rate_headers(headers: &HeaderMap) -> Value {
    let mut result = serde_json::Map::new();
    for name in [
        "retry-after",
        "x-ratelimit-limit-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            result.insert(name.into(), json!(value));
        }
    }
    Value::Object(result)
}

fn retry_delay(status: u16, headers: &HeaderMap, attempt: u32) -> Option<Duration> {
    if attempt >= 2 || (status != 429 && !(500..=599).contains(&status)) {
        return None;
    }
    let seconds = match headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => value.parse::<f64>().ok().or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|date| {
                    (date.with_timezone(&chrono::Utc) - chrono::Utc::now())
                        .num_milliseconds()
                        .max(0) as f64
                        / 1000.0
                })
        })?,
        None => (1u64 << attempt) as f64,
    };
    if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
        return None;
    }
    Some(Duration::from_secs_f64(seconds.max(0.1)))
}

pub(crate) fn send_completion(
    client: &Client,
    request: &Value,
    metrics: &Mutex<GroqHttpMetrics>,
) -> Result<Value> {
    let key = std::env::var("GROQ_API_KEY")
        .map_err(|_| Error::Model("GROQ_API_KEY missing from process environment".into()))?;
    if key.trim().is_empty() {
        return Err(Error::Model("GROQ_API_KEY is empty".into()));
    }
    let started = Instant::now();
    for attempt in 0..=2 {
        metrics
            .lock()
            .map_err(|_| Error::Model("Groq metrics poisoned".into()))?
            .requests += 1;
        let response = client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .bearer_auth(&key)
            .json(request)
            .send()
            .map_err(|error| {
                Error::Model(format!(
                    "PROVIDER_FAILURE: Groq transport: {}",
                    error.to_string().replace(&key, "[REDACTED]")
                ))
            })?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let safe_headers = rate_headers(&headers);
        {
            let mut stats = metrics
                .lock()
                .map_err(|_| Error::Model("Groq metrics poisoned".into()))?;
            stats.last_rate_limits = safe_headers.clone();
            if status == 429 {
                stats.rate_limit_events += 1;
            }
        }
        if !(200..300).contains(&status) {
            if let Some(delay) = retry_delay(status, &headers, attempt) {
                metrics
                    .lock()
                    .map_err(|_| Error::Model("Groq metrics poisoned".into()))?
                    .retries += 1;
                std::thread::sleep(delay);
                continue;
            }
            let body = response
                .text()
                .unwrap_or_default()
                .replace(&key, "[REDACTED]");
            let kind = if status == 429 {
                "RATE_LIMIT_FAILURE"
            } else {
                "PROVIDER_FAILURE"
            };
            return Err(Error::Model(format!(
                "{kind}: Groq HTTP {status}, rate_limits={safe_headers}, error={}",
                body.chars().take(600).collect::<String>()
            )));
        }
        let mut value: Value = response
            .json()
            .map_err(|_| Error::Model("PROVIDER_FAILURE: invalid Groq response JSON".into()))?;
        let elapsed = started.elapsed().as_millis();
        {
            let mut stats = metrics
                .lock()
                .map_err(|_| Error::Model("Groq metrics poisoned".into()))?;
            stats.input_tokens += value["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
            stats.output_tokens += value["usage"]["completion_tokens"].as_u64().unwrap_or(0);
            stats.latency_ms += elapsed;
        }
        value["_anix_transport"] = json!({"requests":attempt+1,"retries":attempt,"latency_ms":elapsed,"rate_limits":safe_headers});
        return Ok(value);
    }
    unreachable!("bounded retry loop returns on final attempt")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_limit_waits_honor_retry_after_and_are_bounded() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "2.5".parse().unwrap());
        assert_eq!(
            retry_delay(429, &headers, 0),
            Some(Duration::from_millis(2500))
        );
        assert_eq!(retry_delay(429, &headers, 2), None);
        assert_eq!(retry_delay(401, &headers, 0), None);
        headers.insert("retry-after", "3600".parse().unwrap());
        assert_eq!(retry_delay(429, &headers, 0), None);
        assert_eq!(
            retry_delay(503, &HeaderMap::new(), 1),
            Some(Duration::from_secs(2))
        );
    }
    #[test]
    fn metrics_headers_cannot_copy_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer synthetic-secret".parse().unwrap());
        headers.insert("x-ratelimit-remaining-tokens", "7999".parse().unwrap());
        assert_eq!(
            rate_headers(&headers),
            json!({"x-ratelimit-remaining-tokens":"7999"})
        );
    }
}
