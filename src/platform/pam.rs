use std::ffi::{CString, c_char, c_int, c_void};

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

type Converse =
    unsafe extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int;

#[repr(C)]
struct Conversation {
    converse: Option<Converse>,
    data: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const Conversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}

unsafe extern "C" {
    fn calloc(count: usize, size: usize) -> *mut c_void;
    fn strdup(text: *const c_char) -> *mut c_char;
}

unsafe extern "C" fn converse(
    count: c_int,
    messages: *mut *const Message,
    responses: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || messages.is_null() || responses.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let size = std::mem::size_of::<Response>();
    let replies = unsafe { calloc(count as usize, size) } as *mut Response;
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    for index in 0..count as usize {
        let message = unsafe { *messages.add(index) };
        if message.is_null() {
            continue;
        }
        let style = unsafe { (*message).style };
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            unsafe { (*replies.add(index)).text = strdup(data as *const c_char) };
        }
    }
    unsafe { *responses = replies };
    PAM_SUCCESS
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

pub fn authenticate(service: &str, user: &str, password: String) -> bool {
    let mut secret = password.into_bytes();
    let verdict = check(service, user, &secret);
    wipe(&mut secret);
    verdict
}

fn check(service: &str, user: &str, secret: &[u8]) -> bool {
    let (Ok(service), Ok(user)) = (CString::new(service), CString::new(user)) else {
        return false;
    };
    let Ok(secret) = CString::new(secret) else {
        return false;
    };
    let mut secret = secret.into_bytes_with_nul();
    let conversation = Conversation {
        converse: Some(converse),
        data: secret.as_mut_ptr() as *mut c_void,
    };
    let mut handle: *mut c_void = std::ptr::null_mut();
    let started = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    let verdict = if started == PAM_SUCCESS {
        let result = unsafe { pam_authenticate(handle, 0) };
        unsafe { pam_end(handle, result) };
        result == PAM_SUCCESS
    } else {
        false
    };
    wipe(&mut secret);
    verdict
}
