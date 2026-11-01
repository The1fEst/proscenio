use std::cell::RefCell;
use std::rc::Rc;

use crate::core::fuzzy::{self, SearchTarget};

pub struct Request {
    pub prompt: String,
    pub placeholder: String,
    pub lines: u32,
    pub is_prompt_only: bool,
    pub items: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Choice(String),
    Cancellation,
}

type Action = Box<dyn FnOnce()>;

#[derive(Clone, Default)]
pub struct AbandonCallback(Rc<RefCell<Option<Action>>>);

impl AbandonCallback {
    pub fn set(&self, action: impl FnOnce() + 'static) {
        self.0.replace(Some(Box::new(action)));
    }

    pub fn invoke(&self) {
        let action = self.0.borrow_mut().take();
        if let Some(action) = action {
            action();
        }
    }
}

pub struct Responder {
    on_reply: Option<Box<dyn FnOnce(Reply)>>,
    on_abandon: AbandonCallback,
}

impl Responder {
    pub fn new(on_reply: impl FnOnce(Reply) + 'static) -> Self {
        Responder {
            on_reply: Some(Box::new(on_reply)),
            on_abandon: AbandonCallback::default(),
        }
    }

    pub fn on_abandon(&self) -> AbandonCallback {
        self.on_abandon.clone()
    }

    pub fn send(mut self, reply: Reply) {
        if let Some(on_reply) = self.on_reply.take() {
            on_reply(reply);
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(on_reply) = self.on_reply.take() {
            on_reply(Reply::Cancellation);
        }
    }
}

struct Session {
    id: u64,
    items: Vec<String>,
    targets: Vec<SearchTarget>,
    is_prompt_only: bool,
    responder: Responder,
}

#[derive(Default)]
pub struct Sessions {
    current: Option<Session>,
    last_id: u64,
}

impl Sessions {
    pub fn begin(&mut self, request: Request, responder: Responder) -> u64 {
        self.cancel();
        self.last_id += 1;
        let targets = request
            .items
            .iter()
            .map(|item| fuzzy::prepare(item))
            .collect();
        self.current = Some(Session {
            id: self.last_id,
            items: request.items,
            targets,
            is_prompt_only: request.is_prompt_only,
            responder,
        });
        self.last_id
    }

    pub fn id(&self) -> u64 {
        self.current.as_ref().map_or(0, |session| session.id)
    }

    pub fn item(&self, index: usize) -> Option<&str> {
        let session = self.current.as_ref()?;
        session.items.get(index).map(String::as_str)
    }

    pub fn search(&self, query: &str, is_sloppy: bool) -> Vec<usize> {
        match &self.current {
            Some(session) if !session.is_prompt_only => {
                fuzzy::rank_lines(&session.items, &session.targets, query, is_sloppy)
            }
            _ => Vec::new(),
        }
    }

    pub fn accept(&mut self, id: u64, index: usize) {
        if id != self.id() {
            return;
        }
        if let Some(item) = self.item(index).map(str::to_owned) {
            self.finish(id, Reply::Choice(item));
        }
    }

    pub fn finish(&mut self, id: u64, reply: Reply) {
        if id == 0 || self.id() != id {
            return;
        }
        if let Some(session) = self.current.take() {
            session.responder.send(reply);
        }
    }

    pub fn cancel(&mut self) -> bool {
        let id = self.id();
        self.finish(id, Reply::Cancellation);
        id != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Replies = Rc<RefCell<Vec<Reply>>>;

    fn build_request(items: &[&str], is_prompt_only: bool) -> Request {
        Request {
            prompt: String::new(),
            placeholder: String::new(),
            lines: 0,
            is_prompt_only,
            items: items.iter().map(|item| (*item).to_owned()).collect(),
        }
    }

    fn build_responder(replies: &Replies) -> Responder {
        let replies = replies.clone();
        Responder::new(move |reply| replies.borrow_mut().push(reply))
    }

    #[test]
    fn a_second_request_cancels_the_first_and_takes_its_place() {
        let first_replies = Replies::default();
        let second_replies = Replies::default();
        let mut sessions = Sessions::default();
        let first = sessions.begin(
            build_request(&["one"], false),
            build_responder(&first_replies),
        );
        let second = sessions.begin(
            build_request(&["two"], false),
            build_responder(&second_replies),
        );
        assert_ne!(first, second);
        assert_eq!(*first_replies.borrow(), [Reply::Cancellation]);
        assert!(second_replies.borrow().is_empty());
        assert_eq!(sessions.id(), second);
        assert_eq!(sessions.item(0), Some("two"));
    }

    #[test]
    fn only_the_current_session_can_be_finished() {
        let replies = Replies::default();
        let mut sessions = Sessions::default();
        let first = sessions.begin(build_request(&["one"], false), build_responder(&replies));
        let second = sessions.begin(build_request(&["two"], false), build_responder(&replies));
        replies.borrow_mut().clear();
        sessions.finish(first, Reply::Choice("one".to_owned()));
        assert!(replies.borrow().is_empty());
        sessions.finish(0, Reply::Cancellation);
        assert!(replies.borrow().is_empty());
        sessions.finish(second, Reply::Choice("typed".to_owned()));
        assert_eq!(*replies.borrow(), [Reply::Choice("typed".to_owned())]);
        assert_eq!(sessions.id(), 0);
    }

    #[test]
    fn accepting_an_item_replies_with_its_text_once() {
        let replies = Replies::default();
        let mut sessions = Sessions::default();
        let session = sessions.begin(
            build_request(&["alpha", "beta"], false),
            build_responder(&replies),
        );
        sessions.accept(session, 1);
        sessions.accept(session, 0);
        sessions.cancel();
        assert_eq!(*replies.borrow(), [Reply::Choice("beta".to_owned())]);
    }

    #[test]
    fn a_row_of_an_earlier_session_cannot_answer_a_later_one() {
        let replies = Replies::default();
        let mut sessions = Sessions::default();
        let earlier_session =
            sessions.begin(build_request(&["alpha"], false), build_responder(&replies));
        sessions.begin(build_request(&["beta"], false), build_responder(&replies));
        replies.borrow_mut().clear();
        sessions.accept(earlier_session, 0);
        assert!(replies.borrow().is_empty());
        assert_ne!(sessions.id(), 0);
    }

    #[test]
    fn an_abandoned_responder_runs_its_callback_once() {
        let replies = Replies::default();
        let responder = build_responder(&replies);
        let on_abandon = responder.on_abandon();
        on_abandon.invoke();
        let count = Rc::new(std::cell::Cell::new(0));
        on_abandon.set({
            let count = count.clone();
            move || count.set(count.get() + 1)
        });
        on_abandon.invoke();
        on_abandon.invoke();
        assert_eq!(count.get(), 1);
        assert!(replies.borrow().is_empty());
    }

    #[test]
    fn cancelling_reports_whether_a_session_was_open() {
        let replies = Replies::default();
        let mut sessions = Sessions::default();
        assert!(!sessions.cancel());
        sessions.begin(build_request(&["alpha"], false), build_responder(&replies));
        assert!(sessions.cancel());
        assert!(!sessions.cancel());
        assert_eq!(*replies.borrow(), [Reply::Cancellation]);
    }

    #[test]
    fn a_responder_dropped_unanswered_cancels() {
        let replies = Replies::default();
        drop(build_responder(&replies));
        assert_eq!(*replies.borrow(), [Reply::Cancellation]);
    }

    #[test]
    fn the_search_filters_the_items_with_the_launcher_matcher() {
        let mut sessions = Sessions::default();
        let items = ["Firefox", "Files", "Kitty", "Firewall"];
        sessions.begin(
            build_request(&items, false),
            build_responder(&Replies::default()),
        );
        assert_eq!(sessions.search("", false), [0, 1, 2, 3]);
        assert_eq!(sessions.search("kit", false), [2]);
        assert_eq!(sessions.search("zzz", false), Vec::<usize>::new());
        assert_eq!(sessions.search("firewall", true)[0], 3);
    }

    #[test]
    fn a_prompt_only_session_lists_nothing() {
        let mut sessions = Sessions::default();
        sessions.begin(
            build_request(&["alpha"], true),
            build_responder(&Replies::default()),
        );
        assert_eq!(sessions.search("", false), Vec::<usize>::new());
        assert_eq!(sessions.search("alpha", false), Vec::<usize>::new());
    }

    #[test]
    fn a_session_keeps_thousands_of_items_in_order() {
        let items: Vec<String> = (0..20_000).map(|number| format!("item {number}")).collect();
        let mut sessions = Sessions::default();
        let request = Request {
            items: items.clone(),
            ..build_request(&[], false)
        };
        sessions.begin(request, build_responder(&Replies::default()));
        assert_eq!(sessions.search("", false).len(), items.len());
        assert_eq!(sessions.search("item 19999", false)[0], 19_999);
        assert_eq!(sessions.item(12_345), Some("item 12345"));
    }
}
