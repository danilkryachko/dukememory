#![no_main]

use dukememory::protocol::validate_egress_url_shape;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(url) = std::str::from_utf8(data) {
        let _ = validate_egress_url_shape(url);
    }
});
