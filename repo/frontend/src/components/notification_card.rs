/// Single notification item within the inbox list.
///
/// Status transitions driven by this component:
///
///   delivered  ──[Acknowledge]──►  read
///              ──[Dismiss]──────►  dismissed (removed from list)
///   queued     ──[Acknowledge]──►  read      (also promoted by receipt endpoint)
///              ──[Dismiss]──────►  dismissed
///   read       ──[Dismiss]──────►  dismissed
///
/// Delivery receipt data shown at the bottom of each card:
///   • "Delivered: <delivered_at>"  — when the backend wrote the delivery row
///   • "Read:      <read_at>"       — when the user acknowledged it (or None)
use uuid::Uuid;
use yew::prelude::*;

use crate::types::notification::Notification;

// ── Props ─────────────────────────────────────────────────────────────────────

#[derive(Properties, PartialEq, Clone)]
pub struct NotificationCardProps {
    pub notification:   Notification,
    /// Called with the delivery ID when the user clicks "Acknowledge".
    pub on_acknowledge: Callback<Uuid>,
    /// Called with the delivery ID when the user clicks "Dismiss".
    pub on_dismiss:     Callback<Uuid>,
}

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(NotificationCard)]
pub fn notification_card(props: &NotificationCardProps) -> Html {
    let n = &props.notification;

    let on_ack = {
        let cb = props.on_acknowledge.clone();
        let id = n.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(id);
        })
    };

    let on_dismiss = {
        let cb = props.on_dismiss.clone();
        let id = n.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(id);
        })
    };

    let card_class = format!(
        "notification-card notification-card--{} {}",
        n.severity_class(),
        if n.is_unread() { "notification-card--unread" }
        else if n.is_queued() { "notification-card--queued" }
        else { "" }
    );

    html! {
        <article class={card_class} role="listitem">

            // ── Header ─────────────────────────────────────────────────────
            <header class="notification-card__header">
                <span class="notification-card__severity-icon" aria-hidden="true">
                    { n.severity_icon() }
                </span>

                <div class="notification-card__meta">
                    <span class="notification-card__category">{ n.category() }</span>
                    <span class="notification-card__type">{ &n.event_type }</span>
                </div>

                // Unread dot
                if n.is_unread() || n.is_queued() {
                    <span
                        class="notification-card__unread-dot"
                        aria-label={if n.is_queued() { "Queued during DND" } else { "Unread" }}
                        title={if n.is_queued() { "Held during DND — now delivered" } else { "Unread" }}
                    />
                }

                // Close / Dismiss button (always available)
                <button
                    class="notification-card__btn-dismiss"
                    onclick={on_dismiss.clone()}
                    aria-label="Dismiss notification"
                    title="Dismiss"
                >
                    { "✕" }
                </button>
            </header>

            // ── Body ───────────────────────────────────────────────────────
            <div class="notification-card__body">
                <p class="notification-card__title">{ n.title() }</p>
                if let Some(msg) = n.message() {
                    <p class="notification-card__message">{ msg }</p>
                }
            </div>

            // ── Delivery receipt footer ────────────────────────────────────
            <footer class="notification-card__footer">
                <div class="notification-card__receipts">
                    <span
                        class="notification-card__receipt"
                        title="When the server delivered this notification"
                    >
                        { "Delivered: " }
                        <time>
                        {
                            n.formatted_delivered_at()
                                .unwrap_or_else(|| "—".to_string())
                        }
                        </time>
                    </span>

                    if let Some(read_time) = n.formatted_read_at() {
                        <span
                            class="notification-card__receipt notification-card__receipt--read"
                            title="When you acknowledged this notification"
                        >
                            { "Read: " }
                            <time>{ read_time }</time>
                        </span>
                    }
                </div>

                // Acknowledge button — only shown for unread/queued items
                if n.is_unread() || n.is_queued() {
                    <button
                        class="notification-card__btn-ack"
                        onclick={on_ack}
                        aria-label="Acknowledge notification"
                    >
                        { "Acknowledge" }
                    </button>
                }
            </footer>
        </article>
    }
}
