use gtk4::gio;
use gtk4::glib::{self, VariantTy};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const BUS: &str = "net.reactivated.Fprint";
const MANAGER: &str = "/net/reactivated/Fprint/Manager";
const MANAGER_INTERFACE: &str = "net.reactivated.Fprint.Manager";
const DEVICE: &str = "net.reactivated.Fprint.Device";
const TIMEOUT: i32 = 5000;
const MAX_TRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub struct Reader {
    system: gio::DBusConnection,
    device: String,
    user: String,
    status: RefCell<Option<gio::SignalSubscription>>,
    misses: Cell<u32>,
    stopped: Cell<bool>,
}

impl Reader {
    pub async fn open(system: &gio::DBusConnection, user: &str) -> Option<Rc<Self>> {
        let reply = system
            .call_future(
                Some(BUS),
                MANAGER,
                MANAGER_INTERFACE,
                "GetDefaultDevice",
                None,
                Some(VariantTy::new("(o)").ok()?),
                gio::DBusCallFlags::NONE,
                TIMEOUT,
            )
            .await
            .ok()?;
        let device = reply.child_value(0).str()?.to_owned();
        let fingers = call(
            system,
            &device,
            "ListEnrolledFingers",
            Some((user,).to_variant()),
        )
        .await?
        .child_value(0);
        if fingers.n_children() == 0 {
            return None;
        }
        Some(Rc::new(Reader {
            system: system.clone(),
            device,
            user: user.to_owned(),
            status: RefCell::new(None),
            misses: Cell::new(0),
            stopped: Cell::new(false),
        }))
    }

    pub async fn verify(self: &Rc<Self>, matched: impl Fn() + 'static) {
        if call(
            &self.system,
            &self.device,
            "Claim",
            Some((&self.user,).to_variant()),
        )
        .await
        .is_none()
        {
            return;
        }
        if self.stopped.get() {
            call(&self.system, &self.device, "Release", None).await;
            return;
        }
        let reader = Rc::downgrade(self);
        self.status.replace(Some(self.system.subscribe_to_signal(
            Some(BUS),
            Some(DEVICE),
            Some("VerifyStatus"),
            Some(&self.device),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some((result, done)) = signal.parameters.get::<(String, bool)>() else {
                    return;
                };
                let Some(reader) = reader.upgrade() else {
                    return;
                };
                if !done || reader.stopped.get() {
                    return;
                }
                match result.as_str() {
                    "verify-match" => {
                        reader.stop();
                        matched();
                    }
                    "verify-no-match" => {
                        reader.misses.set(reader.misses.get() + 1);
                        if reader.misses.get() >= MAX_TRIES {
                            reader.stop();
                        } else {
                            restart(&reader, Duration::ZERO);
                        }
                    }
                    _ => restart(&reader, RETRY_DELAY),
                }
            },
        )));
        if self.stopped.get() {
            return;
        }
        begin(self).await;
    }

    pub fn stop(&self) {
        if self.stopped.replace(true) {
            return;
        }
        self.status.take();
        let system = self.system.clone();
        let device = self.device.clone();
        glib::spawn_future_local(async move {
            call(&system, &device, "VerifyStop", None).await;
            call(&system, &device, "Release", None).await;
        });
    }
}

fn restart(reader: &Rc<Reader>, delay: Duration) {
    let system = reader.system.clone();
    let device = reader.device.clone();
    let reader = Rc::downgrade(reader);
    glib::spawn_future_local(async move {
        call(&system, &device, "VerifyStop", None).await;
        glib::timeout_future(delay).await;
        if let Some(reader) = reader.upgrade().filter(|reader| !reader.stopped.get()) {
            begin(&reader).await;
        }
    });
}

async fn begin(reader: &Rc<Reader>) {
    if call(
        &reader.system,
        &reader.device,
        "VerifyStart",
        Some(("any",).to_variant()),
    )
    .await
    .is_none()
    {
        reader.stop();
    }
}

async fn call(
    system: &gio::DBusConnection,
    device: &str,
    method: &str,
    parameters: Option<glib::Variant>,
) -> Option<glib::Variant> {
    system
        .call_future(
            Some(BUS),
            device,
            DEVICE,
            method,
            parameters.as_ref(),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
        )
        .await
        .ok()
}
