//! Golden values exported for the `@umari/js` test suite. Run with:
//!
//!     cargo test -p umari --test cross_lang_goldens -- --nocapture
//!
//! Copy the printed JSON into `packages/js/test/goldens.json` if any of
//! these algorithms change.

use std::collections::BTreeMap;

use umari::IDEMPOTENCY_NAMESPACE;
use uuid::Uuid;

#[test]
fn print_idempotency_goldens() {
    let cases = [
        (
            "00000000-0000-0000-0000-000000000000",
            "00000000-0000-0000-0000-000000000000",
            0,
        ),
        (
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            0,
        ),
        (
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            1,
        ),
        (
            "aabbccdd-eeff-1122-3344-556677889900",
            "ffeeddcc-bbaa-9988-7766-554433221100",
            7,
        ),
    ];
    let mut out = Vec::new();
    for (corr, caus, idx) in cases {
        let cor = Uuid::parse_str(corr).unwrap();
        let cau = Uuid::parse_str(caus).unwrap();
        let mut key = Vec::with_capacity(16 + 16 + 4);
        key.extend_from_slice(cor.as_bytes());
        key.extend_from_slice(cau.as_bytes());
        key.extend_from_slice(&(idx as u32).to_be_bytes());
        let id = Uuid::new_v5(&IDEMPOTENCY_NAMESPACE, &key);
        out.push(BTreeMap::from([
            ("correlation_id".to_string(), corr.to_string()),
            ("causation_id".to_string(), caus.to_string()),
            ("index".to_string(), idx.to_string()),
            ("expected".to_string(), id.to_string()),
        ]));
    }
    println!(
        "IDEMPOTENCY_GOLDENS = {}",
        serde_json::to_string_pretty(&out).unwrap()
    );
}
