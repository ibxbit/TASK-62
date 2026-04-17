/// Main inbox panel — opens as a slide-in drawer from the notification badge.
///
/// Data flow inside this component
/// ─────────────────────────────────
///
///   [panel opens]              [filter changes]
///        │                           │
///        └──────────┬────────────────┘
///                   ▼
///           load_notifications()
///               │
///               ├─ GET /notifications?status={filter}
///               ├─ POST /notifications/receipt  ← promotes queued → delivered
///               │   (server side-effect: user is online, DND bypassed)
///               └─ GET /notifications/unread-count  ← refresh badge
///               │
///               └─► dispatch SetNotifications + SetCounts
///
///   [Acknowledge click]
///       ├─ dispatch Acknowledge(id)   ← optimistic UI update
///       └─ POST /notifications/{id}/read
///
///   [Dismiss click]
///       ├─ dispatch Dismiss(id)       ← optimistic UI update
///       └─ POST /notifications/{id}/dismiss
///
///   [Mark All Read click]
///       ├─ dispatch AcknowledgeAll    ← optimistic UI update
///       └─ POST /notifications/read-all
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    components::notification_card::NotificationCard,
    services::notification_service,
    store::notification_store::{NotificationAction, NotificationContext},
    types::notification::StatusFilter,
};

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(InboxPanel)]
pub fn inbox_panel() -> Html {
    let ctx = use_context::<NotificationContext>().expect("NotificationContext missing");

    // ── Load notifications ───────────────────────────────────────────────────

    // Reusable fetch closure — reads current filter from context state.
    let fetch = {
        let ctx = ctx.clone();
        Callback::from(move |_: ()| {
            let ctx = ctx.clone();
            spawn_local(async move {
                ctx.dispatch(NotificationAction::SetLoading(true));

                let status = ctx.state.filter.as_query_param();

                match notification_service::fetch_notifications(status, 50, 0).await {
                    Ok(notifications) => {
                        // ── Delivery receipt ──────────────────────────────
                        // Inform the server which notifications the user is
                        // currently viewing. This promotes any queued ones to
                        // 'delivered' so the user sees DND-held items when
                        // they actively open their inbox.
                        let ids: Vec<Uuid> = notifications.iter().map(|n| n.id).collect();
                        if !ids.is_empty() {
                            let _ = notification_service::send_receipt(ids).await;
                        }

                        // Refresh badge counts after receipt promotion
                        if let Ok(counts) = notification_service::fetch_unread_count().await {
                            ctx.dispatch(NotificationAction::SetCounts {
                                unread: counts.unread,
                                queued: counts.queued,
                            });
                        }

                        ctx.dispatch(NotificationAction::SetNotifications(notifications));
                    }
                    Err(e) => {
                        ctx.dispatch(NotificationAction::SetError(Some(e.to_string())));
                    }
                }
            });
        })
    };

    // Fetch when panel opens
    {
        let fetch = fetch.clone();
        let is_open = ctx.state.is_open;
        use_effect_with(is_open, move |&open| {
            if open {
                fetch.emit(());
            }
        });
    }

    // Re-fetch when filter changes (only while panel is open)
    {
        let fetch = fetch.clone();
        let filter = ctx.state.filter.clone();
        let is_open = ctx.state.is_open;
        use_effect_with((filter, is_open), move |(_, open)| {
            if *open {
                fetch.emit(());
            }
        });
    }

    // ── User actions ─────────────────────────────────────────────────────────

    let on_acknowledge = {
        let ctx = ctx.clone();
        Callback::from(move |id: Uuid| {
            let ctx = ctx.clone();
            // Optimistic update — the UI reflects the change immediately
            ctx.dispatch(NotificationAction::Acknowledge(id));
            // Fire-and-forget API call
            spawn_local(async move {
                if let Err(e) = notification_service::acknowledge(id).await {
                    log::error!("acknowledge failed: {}", e);
                }
            });
        })
    };

    let on_dismiss = {
        let ctx = ctx.clone();
        Callback::from(move |id: Uuid| {
            let ctx = ctx.clone();
            ctx.dispatch(NotificationAction::Dismiss(id));
            spawn_local(async move {
                if let Err(e) = notification_service::dismiss(id).await {
                    log::error!("dismiss failed: {}", e);
                }
            });
        })
    };

    let on_ack_all = {
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| {
            let ctx = ctx.clone();
            ctx.dispatch(NotificationAction::AcknowledgeAll);
            spawn_local(async move {
                if let Err(e) = notification_service::acknowledge_all().await {
                    log::error!("acknowledge_all failed: {}", e);
                }
            });
        })
    };

    let on_close = {
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| {
            ctx.dispatch(NotificationAction::Close);
        })
    };

    // ── Filter tab clicks ────────────────────────────────────────────────────

    let set_filter = |filter: StatusFilter| {
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| {
            ctx.dispatch(NotificationAction::SetFilter(filter.clone()));
        })
    };

    // ── Render ────────────────────────────────────────────────────────────────

    if !ctx.state.is_open {
        return html! {};
    }

    let state    = &ctx.state;
    let has_unread = state.notifications.iter().any(|n| n.is_unread());

    html! {
        // Overlay — click outside to close
        <div class="inbox-overlay" onclick={on_close.clone()} aria-modal="true">

            // Panel — stop propagation so clicks inside don't close
            <aside
                class="inbox-panel"
                onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
                aria-label="Notification inbox"
            >
                // ── Header ─────────────────────────────────────────────────
                <header class="inbox-panel__header">
                    <h2 class="inbox-panel__title">{"Notifications"}</h2>
                    <div class="inbox-panel__header-actions">
                        if has_unread {
                            <button
                                class="inbox-panel__btn-ack-all"
                                onclick={on_ack_all}
                                title="Mark all as read"
                            >
                                { "✓ Mark all read" }
                            </button>
                        }
                        <button
                            class="inbox-panel__btn-close"
                            onclick={on_close}
                            aria-label="Close inbox"
                        >
                            { "✕" }
                        </button>
                    </div>
                </header>

                // ── Filter tabs ────────────────────────────────────────────
                <nav class="inbox-panel__tabs" aria-label="Filter notifications">
                    {
                        [StatusFilter::Unread, StatusFilter::Queued, StatusFilter::All]
                            .iter()
                            .map(|f| {
                                let is_active = *f == state.filter;
                                let label     = f.label();
                                let count     = match f {
                                    StatusFilter::Unread => Some(state.unread_count),
                                    StatusFilter::Queued => Some(state.queued_count),
                                    StatusFilter::All    => None,
                                };

                                html! {
                                    <button
                                        key={label}
                                        class={format!(
                                            "inbox-panel__tab {}",
                                            if is_active { "inbox-panel__tab--active" } else { "" }
                                        )}
                                        onclick={set_filter(f.clone())}
                                        aria-selected={is_active.to_string()}
                                        aria-label={format!("{}{}", label, count.map(|c| format!(" ({})", c)).unwrap_or_default())}
                                    >
                                        { label }
                                        if let Some(c) = count {
                                            if c > 0 {
                                                <span class="inbox-panel__tab-badge">{ c }</span>
                                            }
                                        }
                                    </button>
                                }
                            })
                            .collect::<Html>()
                    }
                </nav>

                // ── Body ───────────────────────────────────────────────────
                <div class="inbox-panel__body" role="list" aria-live="polite" aria-busy={state.loading.to_string()}>

                    if state.loading {
                        <div class="inbox-panel__loading" aria-label="Loading notifications">
                            <span class="spinner" aria-hidden="true" />
                            { "Loading…" }
                        </div>
                    } else if let Some(err) = &state.error {
                        <div class="inbox-panel__error" role="alert">
                            <strong>{ "Error: " }</strong>{ err }
                        </div>
                    } else if state.notifications.is_empty() {
                        <div class="inbox-panel__empty">
                            <span aria-hidden="true">{ "📭" }</span>
                            <p>{ "No notifications" }</p>
                        </div>
                    } else {
                        { for state.notifications.iter().map(|n| html! {
                            <NotificationCard
                                key={ n.id.to_string() }
                                notification={ n.clone() }
                                on_acknowledge={ on_acknowledge.clone() }
                                on_dismiss={ on_dismiss.clone() }
                            />
                        }) }
                    }
                </div>

                // ── Footer ──────────────────────────────────────────────────
                <footer class="inbox-panel__footer">
                    <span class="inbox-panel__summary">
                        {
                            if state.unread_count > 0 {
                                format!("{} unread", state.unread_count)
                            } else {
                                "All caught up".to_string()
                            }
                        }
                        { " · " }
                        {
                            if state.queued_count > 0 {
                                format!("{} held during DND", state.queued_count)
                            } else {
                                "No queued items".to_string()
                            }
                        }
                    </span>
                </footer>
            </aside>
        </div>
    }
}
