//! Live sync notices. A signed-in device holds `GET /sync/events` open as a
//! server-sent event stream. When one of the user's devices (or the REST API)
//! writes, the others get a `changed` event and run an ordinary sync round. The
//! stream carries no task data: if it drops or misses a notice, the next round
//! still brings everything.
//!
//! Events: `ready` once on connect, `changed` after another device's write. A
//! keep-alive comment every 20 s keeps proxies and tunnels from closing it, and
//! lets the device tell a dead connection from a quiet one.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Mutex;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use tokio::sync::{broadcast, watch};

/// How often an idle stream sends a keep-alive comment.
pub const KEEP_ALIVE: Duration = Duration::from_secs(20);

/// Writes waiting for a slow listener before it skips ahead; one notice covers any number.
const BACKLOG: usize = 16;

/// A write by `from` (a device id, see auth::AuthUser).
#[derive(Clone, Debug)]
struct Notice {
    from: String,
}

pub struct Hub {
    users: Mutex<HashMap<i64, broadcast::Sender<Notice>>>,
    closing: watch::Sender<bool>,
}

impl Default for Hub {
    fn default() -> Self {
        Hub { users: Mutex::new(HashMap::new()), closing: watch::Sender::new(false) }
    }
}

impl Hub {
    /// Tells the user's other devices that something changed.
    pub fn notify(&self, user_id: i64, from_device: &str) {
        let mut users = self.users.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = users.get(&user_id) {
            // Fails only when nobody listens any more.
            if tx.send(Notice { from: from_device.to_string() }).is_err() {
                users.remove(&user_id);
            }
        }
    }

    /// Ends every stream, so a graceful shutdown doesn't wait on them forever.
    pub fn close(&self) {
        self.closing.send_replace(true);
    }

    /// The stream for one device: everything but its own writes.
    pub fn stream(&self, user_id: i64, device_id: String) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let rx = {
            let mut users = self.users.lock().unwrap_or_else(|e| e.into_inner());
            users.entry(user_id).or_insert_with(|| broadcast::channel(BACKLOG).0).subscribe()
        };
        let listener = Listener { rx, closing: self.closing.subscribe(), device_id, greeted: false };
        let stream = futures_util::stream::unfold(listener, |mut l| async move {
            let event = l.next().await?;
            Some((Ok(event), l))
        });
        Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
    }
}

struct Listener {
    rx: broadcast::Receiver<Notice>,
    closing: watch::Receiver<bool>,
    device_id: String,
    greeted: bool,
}

impl Listener {
    /// The next event to send; None ends the stream.
    async fn next(&mut self) -> Option<Event> {
        if !self.greeted {
            self.greeted = true;
            return Some(Event::default().event("ready").data("{}"));
        }
        loop {
            if *self.closing.borrow() {
                return None;
            }
            tokio::select! {
                // Closing, or the hub is gone.
                _ = self.closing.changed() => return None,
                notice = self.rx.recv() => match notice {
                    Ok(n) if n.from == self.device_id => continue,
                    // Lagged: it missed some, and one notice covers them all.
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        return Some(Event::default().event("changed").data("{}"));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(hub: &Hub, user_id: i64, device: &str) -> Listener {
        let rx = hub.users.lock().unwrap().entry(user_id).or_insert_with(|| broadcast::channel(BACKLOG).0).subscribe();
        Listener { rx, closing: hub.closing.subscribe(), device_id: device.into(), greeted: true }
    }

    async fn soon(l: &mut Listener) -> Option<Option<Event>> {
        tokio::time::timeout(Duration::from_millis(50), l.next()).await.ok()
    }

    #[tokio::test]
    async fn notices_reach_the_users_other_devices_only() {
        let hub = Hub::default();
        let mut phone = listener(&hub, 1, "phone");
        let mut pc = listener(&hub, 1, "pc");
        let mut other_user = listener(&hub, 2, "x");
        hub.notify(1, "pc");
        assert!(matches!(soon(&mut phone).await, Some(Some(_))));
        assert!(soon(&mut pc).await.is_none());
        assert!(soon(&mut other_user).await.is_none());
    }

    #[tokio::test]
    async fn close_ends_the_streams() {
        let hub = Hub::default();
        let mut phone = listener(&hub, 1, "phone");
        hub.close();
        assert!(matches!(soon(&mut phone).await, Some(None)));
    }

    #[test]
    fn a_user_with_no_listeners_is_forgotten() {
        let hub = Hub::default();
        drop(listener(&hub, 1, "phone"));
        hub.notify(1, "pc");
        assert!(hub.users.lock().unwrap().is_empty());
    }
}
