use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use http_body_util::{BodyExt, Full};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::debug;
use umadb_client::AsyncUmaDbClient;
use umadb_dcb::{DcbEvent, DcbEventStoreAsync, DcbQuery, DcbQueryItem};
use umari_core::{
    emit::encode_with_envelope,
    event::{EventEnvelope, StoredEventData},
};
use uuid::Uuid;
use wasmtime_wasi_http::p2::{
    HttpResult, WasiHttpHooks,
    body::{HyperIncomingBody, HyperOutgoingBody},
    types::{HostFutureIncomingResponse, IncomingResponse, OutgoingRequestConfig},
};

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

pub const HTTP_COMPLETED_EVENT_TYPE: &str = "umari.effect.http.completed";

#[derive(Debug, Serialize, Deserialize)]
pub struct HttpCompletedData {
    pub invocation_id: String,
    /// key used for cache lookup: user-provided idempotency-key value, or hex(request_hash)
    pub cache_key: String,
    pub module_version: Version,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    /// base64-encoded request body
    pub request_body: String,
    /// hex-encoded SHA-256 of method+url+headers(excluding idempotency-key)+body
    pub request_hash: String,
    pub injected_idempotency_key: Option<String>,
    pub status: u16,
    pub response_headers: Vec<(String, String)>,
    /// base64-encoded response body
    pub response_body: String,
    pub started_at: DateTime<Utc>,
}

pub struct CachedHttpCall {
    pub cache_key: String,
    pub request_hash: Vec<u8>,
    pub status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Vec<u8>,
}

pub struct EffectJournal {
    pub event_store: Arc<AsyncUmaDbClient>,
    pub effect_name: Arc<str>,
    pub module_version: Version,
    pub invocation_id: String,
    pub triggering_event_id: Uuid,
    pub triggering_event_position: u64,
    pub correlation_id: Uuid,
    pub replay_cache: Arc<Mutex<HashMap<String, VecDeque<CachedHttpCall>>>>,
    pub seen_cache_keys: Arc<Mutex<HashSet<String>>>,
}

impl WasiHttpHooks for EffectJournal {
    fn send_request(
        &mut self,
        request: hyper::Request<HyperOutgoingBody>,
        config: OutgoingRequestConfig,
    ) -> HttpResult<HostFutureIncomingResponse> {
        let event_store = Arc::clone(&self.event_store);
        let invocation_id = self.invocation_id.clone();
        let module_version = self.module_version.clone();
        let effect_name = self.effect_name.to_string();
        let correlation_id = self.correlation_id;
        let triggering_event_id = self.triggering_event_id;
        let replay_cache = Arc::clone(&self.replay_cache);
        let seen_cache_keys = Arc::clone(&self.seen_cache_keys);

        let handle = wasmtime_wasi::runtime::spawn(async move {
            let started_at = Utc::now();
            let (mut parts, body) = request.into_parts();

            let body_bytes = body
                .collect()
                .await
                .map_err(|err| wasmtime::format_err!("failed to collect request body: {err:?}"))?
                .to_bytes();

            if body_bytes.len() > MAX_BODY_BYTES {
                return Err(wasmtime::format_err!("request body exceeds 5 MB limit"));
            }

            let method = parts.method.as_str().to_string();
            let url = parts.uri.to_string();

            let all_headers: Vec<(String, String)> = parts
                .headers
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let user_idempotency_key = all_headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("idempotency-key"))
                .map(|(_, v)| v.clone());

            // exclude idempotency-key from hash so it doesn't affect cache lookup
            let stable_headers: Vec<(String, String)> = all_headers
                .iter()
                .filter(|(k, _)| !k.eq_ignore_ascii_case("idempotency-key"))
                .cloned()
                .collect();

            let request_hash = compute_request_hash(&method, &url, &stable_headers, &body_bytes);

            let cache_key = match &user_idempotency_key {
                Some(key) => key.clone(),
                None => hex::encode(&request_hash),
            };

            // trap on duplicate idempotency key within this invocation — unambiguously a bug
            {
                let mut seen = seen_cache_keys.lock().unwrap();
                if !seen.insert(cache_key.clone()) {
                    return Err(wasmtime::format_err!(
                        "duplicate idempotency key within invocation: {cache_key}"
                    ));
                }
            }

            // check replay cache
            let cached = {
                let mut cache = replay_cache.lock().unwrap();
                cache.get_mut(&cache_key).and_then(|q| q.pop_front())
            };

            if let Some(cached) = cached {
                if request_hash != cached.request_hash {
                    return Err(wasmtime::format_err!(
                        "determinism error for cache key {cache_key}: request hash mismatch"
                    ));
                }

                debug!(%invocation_id, cache_key, "replaying http call from cache");

                let resp = build_response(
                    cached.status,
                    cached.response_headers,
                    cached.response_body,
                    config.between_bytes_timeout,
                )
                .map_err(|err| wasmtime::format_err!("{err}"))?;
                return Ok(Ok(resp));
            }

            // cache miss: inject idempotency key and execute live
            let injected_idempotency_key = if user_idempotency_key.is_none() {
                let key = compute_injected_idempotency_key(&invocation_id, &cache_key);
                parts.headers.insert(
                    hyper::header::HeaderName::from_bytes(b"idempotency-key").unwrap(),
                    hyper::header::HeaderValue::from_str(&key).unwrap(),
                );
                Some(key)
            } else {
                None
            };

            let new_body: HyperOutgoingBody = Full::new(body_bytes.clone())
                .map_err(|err: std::convert::Infallible| match err {})
                .boxed_unsync();
            let new_request = hyper::Request::from_parts(parts, new_body);

            let incoming =
                match wasmtime_wasi_http::p2::default_send_request_handler(new_request, config)
                    .await
                {
                    Ok(r) => r,
                    // network-level error: do not journal, return error to guest
                    Err(err_code) => return Ok(Err(err_code)),
                };

            let status = incoming.resp.status().as_u16();
            let between_bytes_timeout = incoming.between_bytes_timeout;
            let (resp_parts, resp_body) = incoming.resp.into_parts();
            let resp_headers: Vec<(String, String)> = resp_parts
                .headers
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let resp_body_bytes = resp_body
                .collect()
                .await
                .map_err(|err| {
                    wasmtime::format_err!("failed to collect response body: {err:?}")
                })?
                .to_bytes();

            if resp_body_bytes.len() > MAX_BODY_BYTES {
                return Err(wasmtime::format_err!("response body exceeds 5 MB limit"));
            }

            let data = HttpCompletedData {
                invocation_id: invocation_id.clone(),
                cache_key: cache_key.clone(),
                module_version: module_version.clone(),
                method,
                url,
                request_headers: all_headers,
                request_body: BASE64.encode(&body_bytes),
                request_hash: hex::encode(&request_hash),
                injected_idempotency_key,
                status,
                response_headers: resp_headers.clone(),
                response_body: BASE64.encode(&resp_body_bytes),
                started_at,
            };

            write_http_completed(
                &event_store,
                data,
                correlation_id,
                triggering_event_id,
                &invocation_id,
                &effect_name,
            )
            .await
            .map_err(|err| wasmtime::format_err!("failed to journal http call: {err}"))?;

            debug!(%invocation_id, cache_key, %module_version, status, "journaled http call");

            let resp = build_response(
                status,
                resp_headers,
                resp_body_bytes.to_vec(),
                between_bytes_timeout,
            )
            .map_err(|err| wasmtime::format_err!("{err}"))?;
            Ok(Ok(resp))
        });

        Ok(HostFutureIncomingResponse::pending(handle))
    }
}

/// Derives a stable invocation ID from effect name and triggering event ID.
pub fn compute_invocation_id(effect_name: &str, triggering_event_id: Uuid) -> String {
    let mut hasher = Sha256::new();
    hasher.update(effect_name.as_bytes());
    hasher.update(triggering_event_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Derives a per-call idempotency key injected when the guest does not provide one.
fn compute_injected_idempotency_key(invocation_id: &str, cache_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(invocation_id.as_bytes());
    hasher.update(cache_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Hashes the outgoing request for integrity checking on replay.
///
/// The `headers` slice must already exclude any `idempotency-key` header.
fn compute_request_hash(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(method.as_bytes());
    hasher.update(url.as_bytes());
    for (k, v) in headers {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    hasher.update(body);
    hasher.finalize().to_vec()
}

/// Loads journaled HTTP calls for an invocation and groups them by cache key.
pub async fn load_replay_cache(
    event_store: &AsyncUmaDbClient,
    invocation_id: &str,
) -> anyhow::Result<HashMap<String, VecDeque<CachedHttpCall>>> {
    let query = DcbQuery::with_items([DcbQueryItem::new()
        .types([HTTP_COMPLETED_EVENT_TYPE])
        .tags([format!("invocation_id:{invocation_id}")])]);

    let (events, _) = event_store
        .read_with_head(Some(query), None, false, None)
        .await?;

    let mut map: HashMap<String, VecDeque<CachedHttpCall>> = HashMap::new();

    for ev in events {
        let stored: StoredEventData<HttpCompletedData> = serde_json::from_slice(&ev.event.data)
            .context("failed to deserialize http.completed event")?;
        let data = stored.data;
        let cached = CachedHttpCall {
            cache_key: data.cache_key.clone(),
            request_hash: hex::decode(&data.request_hash)
                .context("invalid request_hash hex in http.completed event")?,
            status: data.status,
            response_headers: data.response_headers,
            response_body: BASE64
                .decode(&data.response_body)
                .context("invalid response_body base64 in http.completed event")?,
        };
        map.entry(data.cache_key).or_default().push_back(cached);
    }

    Ok(map)
}

pub async fn write_http_completed(
    event_store: &AsyncUmaDbClient,
    data: HttpCompletedData,
    correlation_id: Uuid,
    triggering_event_id: Uuid,
    invocation_id: &str,
    effect_name: &str,
) -> anyhow::Result<()> {
    let envelope = EventEnvelope {
        timestamp: Utc::now(),
        correlation_id,
        causation_id: Uuid::new_v4(),
        triggering_event_id: Some(triggering_event_id),
        idempotency_key: None,
    };
    let data_value = serde_json::to_value(&data)?;
    let event = DcbEvent {
        event_type: HTTP_COMPLETED_EVENT_TYPE.to_string(),
        tags: vec![
            format!("invocation_id:{invocation_id}"),
            format!("effect:{effect_name}"),
        ],
        data: encode_with_envelope(envelope, data_value),
        uuid: Some(Uuid::new_v4()),
    };
    event_store.append(vec![event], None, None).await?;
    Ok(())
}

fn build_response(
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    between_bytes_timeout: Duration,
) -> anyhow::Result<IncomingResponse> {
    let body: HyperIncomingBody = Full::new(Bytes::from(body))
        .map_err(|err: std::convert::Infallible| match err {})
        .boxed_unsync();

    let mut builder = hyper::Response::builder().status(status);
    for (k, v) in &headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    let resp = builder.body(body).context("failed to build response")?;

    Ok(IncomingResponse {
        resp,
        worker: None,
        between_bytes_timeout,
    })
}
