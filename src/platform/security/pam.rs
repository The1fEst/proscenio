use gtk4::glib::{self, translate::ToGlibPtr};
use gtk4::prelude::*;
use std::ffi::{CStr, CString, c_char, c_int, c_void};

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[derive(Default)]
pub struct Verdict {
    pub is_granted: bool,
    pub message: Option<String>,
}

struct Exchange {
    secret: *const c_char,
    messages: Vec<String>,
}

impl Exchange {
    fn message(&self) -> Option<String> {
        (!self.messages.is_empty()).then(|| self.messages.join("\n"))
    }
}

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
    let exchange = unsafe { &mut *(data as *mut Exchange) };
    for index in 0..count as usize {
        let message = unsafe { *messages.add(index) };
        if message.is_null() {
            continue;
        }
        let (style, text) = unsafe { ((*message).style, (*message).text) };
        match style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                unsafe { (*replies.add(index)).text = strdup(exchange.secret) };
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO if !text.is_null() => {
                let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
                let text = text.trim();
                if !text.is_empty() {
                    exchange.messages.push(text.to_owned());
                }
            }
            _ => {}
        }
    }
    unsafe { *responses = replies };
    PAM_SUCCESS
}

pub fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

pub fn copy_text(buffer: &impl IsA<gtk4::EntryBuffer>) -> String {
    let text = unsafe { gtk4::ffi::gtk_entry_buffer_get_text(buffer.as_ref().to_glib_none().0) };
    if text.is_null() {
        return String::new();
    }
    unsafe { glib::GStr::from_ptr(text) }.as_str().to_owned()
}

pub fn authenticate(service: &str, user: &str, password: String) -> Verdict {
    let mut secret = password.into_bytes();
    let verdict = check(service, user, &secret);
    wipe(&mut secret);
    verdict
}

fn check(service: &str, user: &str, secret: &[u8]) -> Verdict {
    let (Ok(service), Ok(user)) = (CString::new(service), CString::new(user)) else {
        return Verdict::default();
    };
    let Ok(secret) = CString::new(secret) else {
        return Verdict::default();
    };
    let mut secret = secret.into_bytes_with_nul();
    let mut exchange = Exchange {
        secret: secret.as_ptr() as *const c_char,
        messages: Vec::new(),
    };
    let conversation = Conversation {
        converse: Some(converse),
        data: &mut exchange as *mut Exchange as *mut c_void,
    };
    let mut handle: *mut c_void = std::ptr::null_mut();
    let start_status =
        unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    let is_granted = if start_status == PAM_SUCCESS {
        let result = unsafe { pam_authenticate(handle, 0) };
        unsafe { pam_end(handle, result) };
        result == PAM_SUCCESS
    } else {
        false
    };
    wipe(&mut secret);
    Verdict {
        is_granted,
        message: exchange.message(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn free(pointer: *mut c_void);
    }

    fn converse_with(exchange: &mut Exchange, batch: &[(c_int, &CStr)]) -> Vec<Option<String>> {
        let messages: Vec<Message> = batch
            .iter()
            .map(|(style, text)| Message {
                style: *style,
                text: text.as_ptr(),
            })
            .collect();
        let mut pointers: Vec<*const Message> = messages
            .iter()
            .map(|message| message as *const Message)
            .collect();
        let mut replies: *mut Response = std::ptr::null_mut();
        let status = unsafe {
            converse(
                pointers.len() as c_int,
                pointers.as_mut_ptr(),
                &mut replies,
                exchange as *mut Exchange as *mut c_void,
            )
        };
        assert_eq!(status, PAM_SUCCESS);
        let texts = (0..batch.len())
            .map(|index| {
                let text = unsafe { (*replies.add(index)).text };
                (!text.is_null()).then(|| {
                    let reply_text = unsafe { CStr::from_ptr(text) }
                        .to_string_lossy()
                        .into_owned();
                    unsafe { free(text as *mut c_void) };
                    reply_text
                })
            })
            .collect();
        unsafe { free(replies as *mut c_void) };
        texts
    }

    #[test]
    fn the_conversation_answers_prompts_and_keeps_what_pam_says() {
        let mut exchange = Exchange {
            secret: c"hunter2".as_ptr(),
            messages: Vec::new(),
        };
        let replies = [
            converse_with(
                &mut exchange,
                &[(
                    PAM_TEXT_INFO,
                    c"The account is locked due to 3 failed logins.",
                )],
            ),
            converse_with(
                &mut exchange,
                &[(PAM_ERROR_MSG, c"(10 minutes left to unlock) ")],
            ),
            converse_with(
                &mut exchange,
                &[(PAM_TEXT_INFO, c"  "), (PAM_PROMPT_ECHO_OFF, c"Password: ")],
            ),
        ];
        assert_eq!(
            replies,
            [
                vec![None],
                vec![None],
                vec![None, Some("hunter2".to_owned())]
            ]
        );
        assert_eq!(
            exchange.message().as_deref(),
            Some("The account is locked due to 3 failed logins.\n(10 minutes left to unlock)")
        );
    }

    #[test]
    fn a_conversation_without_messages_has_no_message() {
        let mut exchange = Exchange {
            secret: c"hunter2".as_ptr(),
            messages: Vec::new(),
        };
        converse_with(&mut exchange, &[(PAM_PROMPT_ECHO_ON, c"Login: ")]);
        assert_eq!(exchange.message(), None);
    }
}
