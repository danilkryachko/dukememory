#![recursion_limit = "256"]
mod app;
mod application;
mod build_info;
mod domain;
mod http_api;
mod operation_catalog;
mod runtime_config;
mod storage;

fn main() -> anyhow::Result<()> {
    app::run()
}
