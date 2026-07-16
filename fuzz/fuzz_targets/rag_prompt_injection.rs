#![no_main]

use dukememory::rag_security::{looks_like_prompt_injection, rag_chunk_retrieval_allowed};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let suspicious = looks_like_prompt_injection(text);
        assert_eq!(rag_chunk_retrieval_allowed(text), !suspicious);
    }
});
