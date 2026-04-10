/// Global notification state — shared via Yew Context API.
///
/// Architecture
/// ─────────────
///   InboxContextProvider (wraps the app)
///     ├─ use_reducer(NotificationState)      — single source of truth
///     ├─ Interval @ 30 s                     — polls unread-count for badge
///     └─ ContextProvider<NotificationContext> — exposes state + dispatch
///
/// Components consume:
///   let ctx = use_context::<NotificationContext>().unwrap();
///   ctx.state.unread_count        // read
///   ctx.dispatch(action)           // write
///
/// Data flow (open inbox)
/// ──────────────────────
///   InboxPanel mounts / filter changes
///     → SetLoading(true)
///     → GET /notifications?status=…
///     → POST /notifications/receipt  (promotes queued → delivered)
///     → GET /notifications/unread-count  (refresh badge after receipt)
///     → SetNotifications(vec) + SetCounts(…)
use std::rc::Rc;

use chrono::Utc;
use gloo_timers::callback::Interval;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::notification_service,
    types::notification::{Notification, StatusFilter},
};

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Default)]
pub struct NotificationState {
    pub notifications: Vec<Notification>,
    pub unread_count:  i64,
    pub queued_count:  i64,
    pub is_open:       bool,
    pub filter:        StatusFilter,
    pub loading:       bool,
    pub error:         Option<String>,
}

// ── Actions ───────────────────────────────────────────────────────────────────

pub enum NotificationAction {
    /// Replace the entire notification list (after a fetch).
    SetNotifications(Vec<Notification>),
    /// Update badge counters (from the lightweight unread-count endpoint).
    SetCounts { unread: i64, queued: i64 },
    /// Optimistic acknowledge: flip status to 'read' in local state.
    Acknowledge(Uuid),
    /// Optimistic dismiss: remove the item from the local list.
    Dismiss(Uuid),
    /// Optimistic bulk-acknowledge all unread items.
    AcknowledgeAll,
    /// Switch the active inbox filter tab.
    SetFilter(StatusFilter),
    /// Open or close the inbox panel.
    ToggleOpen,
    /// Close the inbox panel (e.g., click-outside handler).
    Close,
    /// Show/hide the loading spinner.
    SetLoading(bool),
    /// Surface an error message to the inbox panel.
    SetError(Option<String>),
}

// ── Reducer ───────────────────────────────────────────────────────────────────

impl Reducible for NotificationState {
    type Action = NotificationAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let mut s = (*self).clone();

        match action {
            NotificationAction::SetNotifications(ns) => {
                s.notifications = ns;
                s.loading = false;
                s.error   = None;
            }

            NotificationAction::SetCounts { unread, queued } => {
                s.unread_count = unread;
                s.queued_count = queued;
            }

            NotificationAction::Acknowledge(id) => {
                let now = Utc::now();
                for n in &mut s.notifications {
                    if n.id == id && n.status == "delivered" {
                        n.status  = "read".to_string();
                        n.read_at = Some(now);
                        s.unread_count = s.unread_count.saturating_sub(1);
                        break;
                    }
                }
            }

            NotificationAction::Dismiss(id) => {
                if let Some(n) = s.notifications.iter().find(|n| n.id == id) {
                    if n.is_unread() {
                        s.unread_count = s.unread_count.saturating_sub(1);
                    }
                }
                s.notifications.retain(|n| n.id != id);
            }

            NotificationAction::AcknowledgeAll => {
                let now = Utc::now();
                for n in &mut s.notifications {
                    if n.status == "delivered" {
                        n.status  = "read".to_string();
                        n.read_at = Some(now);
                    }
                }
                s.unread_count = 0;
            }

            NotificationAction::SetFilter(f) => {
                s.filter = f;
            }

            NotificationAction::ToggleOpen => {
                s.is_open = !s.is_open;
            }

            NotificationAction::Close => {
                s.is_open = false;
            }

            NotificationAction::SetLoading(loading) => {
                s.loading = loading;
            }

            NotificationAction::SetError(err) => {
                s.error   = err;
                s.loading = false;
            }
        }

        s.into()
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub struct NotificationContext {
    pub state: UseReducerHandle<NotificationState>,
}

impl NotificationContext {
    /// Shorthand for dispatching from a component.
    pub fn dispatch(&self, action: NotificationAction) {
        self.state.dispatch(action);
    }
}

// ── Context provider ──────────────────────────────────────────────────────────

#[derive(Properties, PartialEq)]
pub struct InboxContextProviderProps {
    #[prop_or_default]
    pub children: Children,
}

#[function_component(InboxContextProvider)]
pub fn inbox_context_provider(props: &InboxContextProviderProps) -> Html {
    let state = use_reducer(NotificationState::default);

    // Polls unread-count every 30 s for the badge — even when the inbox is closed.
    // The Interval handle is dropped (and the timer cancelled) on unmount.
    {
        let state = state.clone();

        use_effect_with((), move |_| {
            // Initial fetch on mount
            {
                let state = state.clone();
                spawn_local(async move {
                    if let Ok(counts) = notification_service::fetch_unread_count().await {
                        state.dispatch(NotificationAction::SetCounts {
                            unread: counts.unread,
                            queued: counts.queued,
                        });
                    }
                });
            }

            // Recurring poll every 30 s
            let handle = Interval::new(30_000, move || {
                let state = state.clone();
                spawn_local(async move {
                    if let Ok(counts) = notification_service::fetch_unread_count().await {
                        state.dispatch(NotificationAction::SetCounts {
                            unread: counts.unread,
                            queued: counts.queued,
                        });
                    }
                });
            });

            // Return cleanup — drops the Interval, cancelling the timer
            move || drop(handle)
        });
    }

    let ctx = NotificationContext { state };

    html! {
        <ContextProvider<NotificationContext> context={ctx}>
            { for props.children.iter() }
        </ContextProvider<NotificationContext>>
    }
}
