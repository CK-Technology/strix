//! Password hashing and validation utilities using Argon2id.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use crate::{IamError, Result};

/// Minimum password length.
pub const MIN_PASSWORD_LENGTH: usize = 12;

/// Common weak passwords that are always rejected.
const WEAK_PASSWORDS: &[&str] = &[
    "password",
    "password123",
    "admin",
    "administrator",
    "root",
    "letmein",
    "welcome",
    "monkey",
    "dragon",
    "master",
    "qwerty",
    "login",
    "passw0rd",
    "abc123",
    "111111",
    "123456",
    "1234567",
    "12345678",
    "123456789",
    "1234567890",
    "changeme",
    "secret",
    "trustno1",
    "iloveyou",
    "sunshine",
    "princess",
    "football",
    "baseball",
    "soccer",
    "hockey",
    "batman",
    "superman",
];

/// Password validation error details.
#[derive(Debug, Clone)]
pub struct PasswordValidationError {
    pub reasons: Vec<String>,
}

impl std::fmt::Display for PasswordValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Password validation failed: {}", self.reasons.join("; "))
    }
}

/// Validate password strength.
///
/// Requirements:
/// - Minimum 12 characters
/// - At least one uppercase letter
/// - At least one lowercase letter
/// - At least one digit
/// - At least one special character
/// - Not in common weak password list
///
/// Returns Ok(()) if valid, Err with reasons if invalid.
pub fn validate_password(password: &str) -> std::result::Result<(), PasswordValidationError> {
    let mut reasons = Vec::new();

    if password.len() < MIN_PASSWORD_LENGTH {
        reasons.push(format!(
            "must be at least {} characters (got {})",
            MIN_PASSWORD_LENGTH,
            password.len()
        ));
    }

    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        reasons.push("must contain at least one uppercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_ascii_lowercase()) {
        reasons.push("must contain at least one lowercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_ascii_digit()) {
        reasons.push("must contain at least one digit".to_string());
    }

    if !password.chars().any(|c| !c.is_alphanumeric()) {
        reasons.push("must contain at least one special character".to_string());
    }

    let lower = password.to_lowercase();
    // Check for exact match or if the password is primarily a weak password with minor additions
    if WEAK_PASSWORDS.iter().any(|&weak| {
        lower == weak
            || lower.starts_with(weak)
            || lower.ends_with(weak)
            || (lower.len() <= weak.len() + 4 && lower.contains(weak))
    }) {
        reasons.push("contains a common weak password pattern".to_string());
    }

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(PasswordValidationError { reasons })
    }
}

/// Validate password and hash if valid.
///
/// Returns the PHC string format hash which includes the algorithm, parameters, salt, and hash.
/// Returns an error if the password doesn't meet strength requirements.
pub fn validate_and_hash_password(password: &str) -> Result<String> {
    validate_password(password).map_err(|e| IamError::WeakPassword(e.to_string()))?;
    hash_password(password)
}

/// Hash a password using Argon2id without validation.
///
/// Use `validate_and_hash_password` for user-facing password changes.
/// This function is for internal use (e.g., migrations, root user bootstrap).
///
/// Returns the PHC string format hash which includes the algorithm, parameters, salt, and hash.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| IamError::PasswordHash(e.to_string()))?;

    Ok(hash.to_string())
}

/// Verify a password against a stored hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|e| IamError::PasswordHash(e.to_string()))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "My_Secure_Pass123!";
        let hash = hash_password(password).unwrap();

        // Hash should be PHC format
        assert!(hash.starts_with("$argon2"));

        // Correct password should verify
        assert!(verify_password(password, &hash).unwrap());

        // Wrong password should not verify
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_hashes() {
        let password = "Another_Test_Pass1!";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();

        // Same password should produce different hashes (different salts)
        assert_ne!(hash1, hash2);

        // Both should verify correctly
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_validate_strong_password() {
        // Valid password
        assert!(validate_password("MyStr0ng!Secr3t#").is_ok());
        assert!(validate_password("C0mplex@Xyzt9k!m").is_ok());
        assert!(validate_password("H4rd2Gu3ss!@#$").is_ok());
    }

    #[test]
    fn test_validate_too_short() {
        let result = validate_password("Short1!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.reasons
                .iter()
                .any(|r| r.contains("at least 12 characters"))
        );
    }

    #[test]
    fn test_validate_no_uppercase() {
        let result = validate_password("nouppercase123!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("uppercase")));
    }

    #[test]
    fn test_validate_no_lowercase() {
        let result = validate_password("NOLOWERCASE123!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("lowercase")));
    }

    #[test]
    fn test_validate_no_digit() {
        let result = validate_password("NoDigitsHere!!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("digit")));
    }

    #[test]
    fn test_validate_no_special() {
        let result = validate_password("NoSpecialChars123");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("special")));
    }

    #[test]
    fn test_validate_weak_password_list() {
        // Contains "password"
        let result = validate_password("MyPassword123!!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("weak password")));

        // Exact match "admin"
        let result = validate_password("Admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_and_hash_valid() {
        let result = validate_and_hash_password("V3ry$ecure#Pass!");
        assert!(result.is_ok());
        let hash = result.unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_validate_and_hash_invalid() {
        let result = validate_and_hash_password("weak");
        assert!(result.is_err());
    }
}
