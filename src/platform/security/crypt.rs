use std::ffi::{CStr, CString, c_char, c_int, c_ulong};

use crate::platform::security::pam;

#[link(name = "crypt")]
unsafe extern "C" {
    fn crypt_gensalt(
        prefix: *const c_char,
        count: c_ulong,
        random: *const c_char,
        size: c_int,
    ) -> *mut c_char;
    fn crypt(phrase: *const c_char, setting: *const c_char) -> *mut c_char;
}

pub fn hash(password: &str) -> Option<String> {
    let mut phrase = match CString::new(password) {
        Ok(phrase) => phrase.into_bytes_with_nul(),
        Err(error) => {
            pam::wipe(&mut error.into_vec());
            return None;
        }
    };
    let setting = unsafe { crypt_gensalt(std::ptr::null(), 0, std::ptr::null(), 0) };
    let hash = if setting.is_null() {
        std::ptr::null_mut()
    } else {
        unsafe { crypt(phrase.as_ptr() as *const c_char, setting) }
    };
    pam::wipe(&mut phrase);
    if hash.is_null() {
        return None;
    }
    let hash = unsafe { CStr::from_ptr(hash) }.to_str().ok()?.to_owned();
    hash.starts_with('$').then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(password: &str, stored_hash: &str) -> bool {
        let (Ok(phrase), Ok(setting)) = (CString::new(password), CString::new(stored_hash)) else {
            return false;
        };
        let recomputed_hash = unsafe { crypt(phrase.as_ptr(), setting.as_ptr()) };
        !recomputed_hash.is_null()
            && unsafe { CStr::from_ptr(recomputed_hash) }.to_bytes() == stored_hash.as_bytes()
    }

    #[test]
    fn a_hash_is_salted_and_checks_only_its_own_password() {
        let first = hash("correct horse").unwrap();
        let second = hash("correct horse").unwrap();
        assert_ne!(first, second);
        assert!(matches("correct horse", &first));
        assert!(!matches("wrong horse", &first));
    }
}
