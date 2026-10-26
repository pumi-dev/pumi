//! The API level code says it is written for: a plugin's manifest's `api`.
use crate::{API_LEVEL, Error, Result};

/// `level` is the level this pumi speaks, or the error saying why code written for it can't run.
pub fn check(level: u32) -> Result<()> {
    match level {
        _ if level == API_LEVEL => Ok(()),
        0 => Err(Error::invalid_params("API levels start at 1")),
        _ if level > API_LEVEL => Err(Error::invalid_params(format!(
            "written for API level {level}; this pumi speaks up to {API_LEVEL}: update pumi"
        ))),
        _ => Err(Error::invalid_params(format!("written for API level {level}; this pumi speaks {API_LEVEL}"))),
    }
}
