#![no_main]

use dukememory::protocol::read_mcp_content_length_header;
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let _ = read_mcp_content_length_header(&mut Cursor::new(data));
});
