#![no_main]

use dukememory::protocol::parse_sync_payload_json;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_sync_payload_json(data);
});
