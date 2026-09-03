//! A skill name must be exactly one path component, the native
//! `SkillPathValidator` rule: rejecting `..`, embedded separators, hidden
//! names and control characters keeps every resolved path a child of the
//! root it was joined to.

use super::SkillError;

pub fn is_valid(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
        && name.chars().count() <= 255
}

pub fn validate(name: &str) -> Result<(), SkillError> {
    if is_valid(name) {
        Ok(())
    } else {
        Err(SkillError::InvalidDirectoryName(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_safe_segment_only() {
        for good in ["docx", "code-review", "my_skill.v2", "中文"] {
            assert!(is_valid(good), "{good}");
        }
        for bad in ["", ".", "..", ".hidden", "a/b", "a\\b", "tab\tname", "../x"] {
            assert!(!is_valid(bad), "{bad:?}");
        }
    }
}
