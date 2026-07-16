#![no_main]

use dukememory::protocol::http_content_length;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = http_content_length(data);
});
