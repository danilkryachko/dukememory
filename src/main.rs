mod app;
mod build_info;
mod http_api;
mod runtime_config;
mod services;
mod storage;

fn main() -> anyhow::Result<()> {
    app::run()
}
