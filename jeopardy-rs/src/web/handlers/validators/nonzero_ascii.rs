use crate::web::handlers::validators::CredsValidator;

pub fn is_non_zero_ascii_chars(target_string: &str, max_length: usize) -> bool {
    let n = target_string.len();
    let non_zero_length = n > 0;
    let under_length_limit = n <= max_length;
    let visible_ascii = target_string
        .chars()
        .all(|c| c.is_ascii_alphabetic() || c.is_numeric() || c.is_ascii_punctuation());
    non_zero_length && under_length_limit && visible_ascii
}

/// A variant of `CredsValidator` where given a certain max length
/// validates that all characters are ASCII and
/// string length is non-zero but less than or equal to max length
pub struct NonZeroAsciiValidator {
    max_length: usize,
}

impl NonZeroAsciiValidator {
    pub fn new(max_length: usize) -> Self {
        Self { max_length }
    }
    fn is_valid(&self, s: &str) -> bool {
        is_non_zero_ascii_chars(s, self.max_length)
    }
}

impl CredsValidator for NonZeroAsciiValidator {
    fn is_valid_lobby_id(&self, id: &str) -> bool {
        self.is_valid(id)
    }

    fn is_valid_username(&self, username: &str) -> bool {
        self.is_valid(username)
    }

    fn is_valid_host_password(&self, password: &str) -> bool {
        self.is_valid(password)
    }

    fn is_valid_lobby_password(&self, password: &str) -> bool {
        self.is_valid(password)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod nonzero_ascii_tests {
    use crate::web::handlers::validators::{CredsValidator, nonzero_ascii::NonZeroAsciiValidator};


    const TEST_MAX_NAME_LENGTH: usize = 32;

    #[test]
    fn GIVEN_valid_creds_WHEN_NonZeroAsciiValidator_THEN_ok() {
        // GIVEN
        let validator = NonZeroAsciiValidator::new(TEST_MAX_NAME_LENGTH);
        for i in 0..5 {
            let test_str = match i {
                0 => "a".repeat(TEST_MAX_NAME_LENGTH), // max length test
                1 => "abcdefghijklmnopqrstuvwxyz".to_string(), // lowercase test
                2 => "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(), // uppercase test
                3 => "a12345678910_".to_string(), // special char test
                _ => { // combo test
                    let mut s = "this_IS_a_C0MB1N@T10nAAaa[]{}!!!".to_string();
                    while s.len() != TEST_MAX_NAME_LENGTH {
                        s.push('a');
                    }
                    s
                },
            };
            // WHEN / THEN
            assert!(validator.is_valid_lobby_id(&test_str));
            assert!(validator.is_valid_host_password(&test_str));
            assert!(validator.is_valid_lobby_password(&test_str));
            assert!(validator.is_valid_username(&test_str));
        }
    }

    #[test]
    fn GIVEN_invalid_creds_WHEN_NonZeroAsciiValidator_THEN_ok() {
        // GIVEN
        let validator = NonZeroAsciiValidator::new(TEST_MAX_NAME_LENGTH);
        for i in 0..5 {
            let test_str = match i {
                0 => "".to_string(), // empty string
                1 => "a".repeat(TEST_MAX_NAME_LENGTH + 1), // > max length
                2 => "💀".to_string(), // non ascii
                3 => "\t".to_string(), // non-visible ascii
                _ => { // combo test
                    let mut s = "this_IS_a_C0MB1N@T10nAAaa[]{}!!!".to_string();
                    while s.len() != TEST_MAX_NAME_LENGTH+1 { // go over max length
                        s.push(' '); // add whitespace
                    }
                    s
                },
            };
            // WHEN / THEN
            assert_eq!(validator.is_valid_lobby_id(&test_str), false);
            assert_eq!(validator.is_valid_host_password(&test_str), false);
            assert_eq!(validator.is_valid_lobby_password(&test_str), false);
            assert_eq!(validator.is_valid_username(&test_str), false);
        }
    }
}
