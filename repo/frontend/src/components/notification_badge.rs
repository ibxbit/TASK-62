/// Bell icon with unread count badge — placed in the app header.
///
/// Behaviour:
///   • Unread count displayed as a red badge on the bell icon.
///   • Separate muted badge when DND-queued notifications exist.
///   • Clicking the icon dispatches `ToggleOpen`, showing/hiding the InboxPanel.
///   • Pulses (CSS animation class) when the unread count increases.
use yew::prelude::*;

use crate::store::notification_store::{NotificationAction, NotificationContext};

#[function_component(NotificationBadge)]
pub fn notification_badge() -> Html {
    let ctx = use_context::<NotificationContext>().expect("NotificationContext missing");
    let state = &ctx.state;

    let on_click = {
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| {
            ctx.dispatch(NotificationAction::ToggleOpen);
        })
    };

    // Visual state helpers
    let has_unread  = state.unread_count > 0;
    let has_queued  = state.queued_count > 0;
    let is_open     = state.is_open;

    let button_class = format!(
        "inbox-badge {}",
        if is_open { "inbox-badge--active" } else { "" }
    );

    html! {
        <button
            class={button_class}
            onclick={on_click}
            aria-label={format!(
                "Notifications: {} unread{}",
                state.unread_count,
                if has_queued {
                    format!(", {} queued (DND)", state.queued_count)
                } else {
                    String::new()
                }
            )}
            aria-expanded={is_open.to_string()}
            aria-haspopup="true"
        >
            // Bell icon
            <span class="inbox-badge__icon" aria-hidden="true">{"🔔"}</span>

            // Unread count badge (only shown when there are unread notifications)
            if has_unread {
                <span class="inbox-badge__count inbox-badge__count--unread">
                    { if state.unread_count > 99 { "99+".to_string() } else { state.unread_count.to_string() } }
                </span>
            }

            // Queued count badge (muted — user knows these are DND-held)
            if has_queued && !has_unread {
                <span
                    class="inbox-badge__count inbox-badge__count--queued"
                    title={format!("{} notification(s) queued during DND", state.queued_count)}
                >
                    { state.queued_count.to_string() }
                </span>
            }
        </button>
    }
}
