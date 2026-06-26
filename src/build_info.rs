use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BuildInfo {
    pub version: &'static str,
    pub schema: i64,
    pub vec_feature: bool,
    pub os: &'static str,
    pub arch: &'static str,
}

impl BuildInfo {
    pub fn current(schema: i64) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            schema,
            vec_feature: cfg!(feature = "vec"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }
}
