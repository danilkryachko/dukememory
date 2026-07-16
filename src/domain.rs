use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    ProductGoal,
    UserPreference,
    Decision,
    DesignNote,
    KnownIssue,
    Command,
    TaskState,
    DomainFact,
    Constraint,
    Note,
}

impl MemoryType {
    pub const ALL: [Self; 10] = [
        Self::ProductGoal,
        Self::UserPreference,
        Self::Decision,
        Self::DesignNote,
        Self::KnownIssue,
        Self::Command,
        Self::TaskState,
        Self::DomainFact,
        Self::Constraint,
        Self::Note,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProductGoal => "product_goal",
            Self::UserPreference => "user_preference",
            Self::Decision => "decision",
            Self::DesignNote => "design_note",
            Self::KnownIssue => "known_issue",
            Self::Command => "command",
            Self::TaskState => "task_state",
            Self::DomainFact => "domain_fact",
            Self::Constraint => "constraint",
            Self::Note => "note",
        }
    }
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryType {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| DomainParseError::new("memory type", value, Self::expected()))
    }
}

impl MemoryType {
    fn expected() -> String {
        Self::ALL
            .into_iter()
            .map(Self::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    User,
    Project,
    Repo,
    Thread,
    Task,
}

impl MemoryScope {
    pub const ALL: [Self; 6] = [
        Self::Global,
        Self::User,
        Self::Project,
        Self::Repo,
        Self::Thread,
        Self::Task,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::User => "user",
            Self::Project => "project",
            Self::Repo => "repo",
            Self::Thread => "thread",
            Self::Task => "task",
        }
    }

    fn expected() -> String {
        Self::ALL
            .into_iter()
            .map(Self::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryScope {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| DomainParseError::new("scope", value, Self::expected()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Superseded,
    Rejected,
    Uncertain,
}

impl MemoryStatus {
    pub const ALL: [Self; 4] = [
        Self::Active,
        Self::Superseded,
        Self::Rejected,
        Self::Uncertain,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Rejected => "rejected",
            Self::Uncertain => "uncertain",
        }
    }

    fn expected() -> String {
        Self::ALL
            .into_iter()
            .map(Self::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for MemoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryStatus {
    type Err = DomainParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| DomainParseError::new("memory status", value, Self::expected()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainParseError {
    field: &'static str,
    value: String,
    expected: String,
}

impl DomainParseError {
    fn new(field: &'static str, value: &str, expected: String) -> Self {
        Self {
            field,
            value: value.to_string(),
            expected,
        }
    }
}

impl fmt::Display for DomainParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid {}: {}. Expected one of: {}",
            self.field, self.value, self.expected
        )
    }
}

impl Error for DomainParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_values_round_trip_through_strings() {
        for value in MemoryType::ALL {
            assert_eq!(value.as_str().parse::<MemoryType>().unwrap(), value);
        }
        for value in MemoryScope::ALL {
            assert_eq!(value.as_str().parse::<MemoryScope>().unwrap(), value);
        }
        for value in MemoryStatus::ALL {
            assert_eq!(value.as_str().parse::<MemoryStatus>().unwrap(), value);
        }
    }

    #[test]
    fn domain_values_reject_unknown_strings() {
        assert!("other".parse::<MemoryType>().is_err());
        assert!("workspace".parse::<MemoryScope>().is_err());
        assert!("deleted".parse::<MemoryStatus>().is_err());
    }
}
