use serde::Serialize;

pub const MIN_SAFE_SQLITE_VERSION: &str = "3.51.3";
pub const MIN_SAFE_SQLITE_VERSION_NUMBER: i32 = 3_051_003;

pub fn sqlite_runtime_is_safe() -> bool {
    rusqlite::version_number() >= MIN_SAFE_SQLITE_VERSION_NUMBER
}

#[derive(Debug, Serialize)]
pub struct BuildInfo {
    pub version: &'static str,
    pub schema: i64,
    pub vec_feature: bool,
    pub os: &'static str,
    pub arch: &'static str,
    pub sqlite_version: &'static str,
    pub sqlite_version_number: i32,
    pub sqlite_minimum_safe: &'static str,
    pub sqlite_safe: bool,
}

impl BuildInfo {
    pub fn current(schema: i64) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            schema,
            vec_feature: cfg!(feature = "vec"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            sqlite_version: rusqlite::version(),
            sqlite_version_number: rusqlite::version_number(),
            sqlite_minimum_safe: MIN_SAFE_SQLITE_VERSION,
            sqlite_safe: sqlite_runtime_is_safe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sqlite_meets_the_reviewed_wal_safety_floor() {
        let info = BuildInfo::current(0);
        assert!(
            info.sqlite_safe,
            "bundled SQLite {} is below reviewed minimum {}",
            info.sqlite_version, info.sqlite_minimum_safe
        );
    }
}
