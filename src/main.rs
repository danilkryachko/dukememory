#![recursion_limit = "256"]
mod app;
mod application;
mod build_info;
mod domain;
mod http_api;
mod operation_catalog;
mod rag_security;
mod runtime_config;
mod storage;

fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("dukememory-main".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(app::run)?
        .join()
        .map_err(|_| anyhow::anyhow!("dukememory main worker panicked"))?
}
