use std::ffi::{CStr, CString, c_char, c_int, c_ulong};

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
    let phrase = CString::new(password).ok()?;
    let setting = unsafe { crypt_gensalt(std::ptr::null(), 0, std::ptr::null(), 0) };
    if setting.is_null() {
        return None;
    }
    let hashed = unsafe { crypt(phrase.as_ptr(), setting) };
    if hashed.is_null() {
        return None;
    }
    let hashed = unsafe { CStr::from_ptr(hashed) }.to_str().ok()?.to_owned();
    hashed.starts_with('$').then_some(hashed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(password: &str, hashed: &str) -> bool {
        let (Ok(phrase), Ok(setting)) = (CString::new(password), CString::new(hashed)) else {
            return false;
        };
        let again = unsafe { crypt(phrase.as_ptr(), setting.as_ptr()) };
        !again.is_null() && unsafe { CStr::from_ptr(again) }.to_bytes() == hashed.as_bytes()
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
