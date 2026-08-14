//! Bounded password hashing helpers with fixed, reviewed work factors.

use crate::builtins::{PasswordAlgorithm, PasswordOperation};
use crate::decode::Value;
use crate::error::{FastDbError, Result};
use argon2::{Algorithm, Argon2, Params as Argon2Params, Version as Argon2Version};
use password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_HASH_BYTES: usize = 512;

pub(crate) fn evaluate(
    algorithm: PasswordAlgorithm,
    operation: PasswordOperation,
    arguments: &[Value],
) -> Result<Value> {
    match operation {
        PasswordOperation::Generate => generate(algorithm, string(&arguments[0])?),
        PasswordOperation::Compare => {
            compare(algorithm, string(&arguments[0])?, string(&arguments[1])?)
        }
    }
}

fn generate(algorithm: PasswordAlgorithm, password: &str) -> Result<Value> {
    validate_password(algorithm, password)?;
    let salt = SaltString::generate(&mut OsRng);
    let encoded = match algorithm {
        PasswordAlgorithm::Argon2 => argon2()?.hash_password(password.as_bytes(), &salt),
        PasswordAlgorithm::Pbkdf2 => pbkdf2::Pbkdf2.hash_password(password.as_bytes(), &salt),
        PasswordAlgorithm::Scrypt => scrypt::Scrypt.hash_password(password.as_bytes(), &salt),
        PasswordAlgorithm::Bcrypt => {
            return bcrypt::non_truncating_hash(password, 12)
                .map(Value::Str)
                .map_err(|_| FastDbError::Schema("bcrypt generation failed".into()));
        }
    }
    .map_err(|_| FastDbError::Schema("password hash generation failed".into()))?
    .to_string();
    if encoded.len() > MAX_HASH_BYTES {
        return Err(FastDbError::ResourceLimit(
            "password hash exceeds the output limit".into(),
        ));
    }
    Ok(Value::Str(encoded))
}

fn compare(algorithm: PasswordAlgorithm, encoded: &str, password: &str) -> Result<Value> {
    validate_password(algorithm, password)?;
    if encoded.len() > MAX_HASH_BYTES {
        return Err(FastDbError::ResourceLimit(
            "password hash exceeds the input limit".into(),
        ));
    }
    let valid = match algorithm {
        PasswordAlgorithm::Bcrypt => {
            bcrypt::non_truncating_verify(password, encoded).unwrap_or(false)
        }
        PasswordAlgorithm::Argon2 | PasswordAlgorithm::Pbkdf2 | PasswordAlgorithm::Scrypt => {
            let Ok(hash) = PasswordHash::new(encoded) else {
                return Ok(Value::Bool(false));
            };
            match algorithm {
                PasswordAlgorithm::Argon2 => argon2()?
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok(),
                PasswordAlgorithm::Pbkdf2 => pbkdf2::Pbkdf2
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok(),
                PasswordAlgorithm::Scrypt => scrypt::Scrypt
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok(),
                PasswordAlgorithm::Bcrypt => unreachable!(),
            }
        }
    };
    Ok(Value::Bool(valid))
}

fn argon2() -> Result<Argon2<'static>> {
    let parameters = Argon2Params::new(19_456, 2, 1, Some(32))
        .map_err(|_| FastDbError::Engine("invalid fixed Argon2 parameters".into()))?;
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Argon2Version::V0x13,
        parameters,
    ))
}

fn validate_password(algorithm: PasswordAlgorithm, password: &str) -> Result<()> {
    let maximum = if algorithm == PasswordAlgorithm::Bcrypt {
        72
    } else {
        MAX_PASSWORD_BYTES
    };
    if password.len() > maximum {
        Err(FastDbError::ResourceLimit(
            "password exceeds the algorithm input limit".into(),
        ))
    } else {
        Ok(())
    }
}

fn string(value: &Value) -> Result<&str> {
    match value {
        Value::Str(value) => Ok(value),
        _ => Err(FastDbError::Schema(
            "password functions require string arguments".into(),
        )),
    }
}
