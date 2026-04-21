use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::TelesyncError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub publications: Vec<Publication>,
    pub accounts: HashMap<String, AccountConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Publication {
    pub name: String,
    pub path: String,
    pub account: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    pub short_name: String,
    pub author_name: String,
    pub author_url: String,
    pub access_token: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, TelesyncError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            TelesyncError::Config(format!(
                "failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;

        let config: Config = toml::from_str(&content).map_err(|e| {
            TelesyncError::Config(format!(
                "failed to parse config file {}: {}",
                path.display(),
                e
            ))
        })?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), TelesyncError> {
        self.validate_unique_publication_names()?;
        self.validate_account_references()?;
        self.validate_no_overlapping_paths()?;
        self.validate_access_tokens()?;
        self.validate_short_names()?;
        Ok(())
    }

    fn validate_unique_publication_names(&self) -> Result<(), TelesyncError> {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for (i, pub_entry) in self.publications.iter().enumerate() {
            if let Some(&prev_idx) = seen.get(pub_entry.name.as_str()) {
                return Err(TelesyncError::Config(format!(
                    "duplicate publication name '{}' at index {} and {}",
                    pub_entry.name, prev_idx, i
                )));
            }
            seen.insert(&pub_entry.name, i);
        }
        Ok(())
    }

    fn validate_account_references(&self) -> Result<(), TelesyncError> {
        for pub_entry in &self.publications {
            if !self.accounts.contains_key(&pub_entry.account) {
                return Err(TelesyncError::Config(format!(
                    "publication '{}' references unknown account '{}'",
                    pub_entry.name, pub_entry.account
                )));
            }
        }
        Ok(())
    }

    fn validate_no_overlapping_paths(&self) -> Result<(), TelesyncError> {
        let normalized: Vec<(&str, String)> = self
            .publications
            .iter()
            .map(|p| {
                let mut path = p.path.replace('\\', "/");
                if !path.ends_with('/') {
                    path.push('/');
                }
                (p.name.as_str(), path)
            })
            .collect();

        for i in 0..normalized.len() {
            for j in (i + 1)..normalized.len() {
                let (name_a, path_a) = &normalized[i];
                let (name_b, path_b) = &normalized[j];

                if path_a.starts_with(path_b.as_str()) || path_b.starts_with(path_a.as_str()) {
                    return Err(TelesyncError::Config(format!(
                        "publication paths overlap: '{}' ({}) and '{}' ({})",
                        name_a, path_a, name_b, path_b
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_access_tokens(&self) -> Result<(), TelesyncError> {
        for (name, account) in &self.accounts {
            if account.access_token.trim().is_empty() {
                return Err(TelesyncError::Config(format!(
                    "account '{}' has an empty access_token",
                    name
                )));
            }
        }
        Ok(())
    }

    fn validate_short_names(&self) -> Result<(), TelesyncError> {
        for (name, account) in &self.accounts {
            let len = account.short_name.len();
            if len == 0 || len > 32 {
                return Err(TelesyncError::Config(format!(
                    "account '{}' short_name must be 1-32 characters, got {}",
                    name, len
                )));
            }
        }
        Ok(())
    }
}
