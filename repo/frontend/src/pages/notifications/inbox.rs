/// Staff user inbox page (full-page version).
///
/// Mirrors InboxPanel functionality but as a routed page with filter tabs
/// (Unread / Queued / All) and explicit acknowledge-all / dismiss support.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::notification_service,
    types::notification::{Notification, StatusFilter},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<Notification>), Error(String) }

#[function_component(InboxPage)]
pub fn inbox_page() -> Html {
    let filter     = use_state(|| StatusFilter::Unread);
    let page_state = use_state(|| PageState::Loading);

    let load = {
        let ps = page_state.clone();
        let f  = filter.clone();
        move || {
            let ps = ps.clone();
            let f  = f.clone();
            spawn_local(async move {
                ps.set(PageState::Loading);
                let status = (*f).as_query_param();
                match notification_service::fetch_notifications(status, 50, 0).await {
                    Ok(items) => ps.set(PageState::Loaded(items)),
                    Err(e)    => ps.set(PageState::Error(e.to_string())),
                }
            });
        }
    };

    {
        let l = load.clone();
        let f = filter.clone();
        use_effect_with((*f).clone(), move |_| { l(); || () });
    }

    let ack_all = {
        let load = load.clone();
        Callback::from(move |_: MouseEvent| {
            let load = load.clone();
            spawn_local(async move {
                let _ = notification_service::acknowledge_all().await;
                load();
            });
        })
    };

    let dismiss_cb = {
        let load = load.clone();
        Callback::from(move |id: uuid::Uuid| {
            let load = load.clone();
            spawn_local(async move {
                let _ = notification_service::dismiss(id).await;
                load();
            });
        })
    };

    let set_filter = |f: StatusFilter| {
        let filter = filter.clone();
        Callback::from(move |_: MouseEvent| filter.set(f.clone()))
    };

    let tab_class = |f: &StatusFilter| {
        if *f == *filter { "tab tab--active" } else { "tab" }
    };

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state">
                <p>{ "No notifications." }</p>
            </div>
        },
        PageState::Loaded(items) => {
            let dis = dismiss_cb.clone();
            html! {
                <ul class="notification-list">
                    { for items.iter().map(|n| {
                        let nid  = n.id;
                        let dis2 = dis.clone();
                        html! {
                            <li class={format!("notification-card notification--{}", n.severity_class())}
                                key={nid.to_string()}>
                                <div class="notification-card__header">
                                    <span>{ n.title() }</span>
                                    <span class="notification-card__meta">{ n.formatted_created_at() }</span>
                                </div>
                                if let Some(msg) = n.message() {
                                    <p class="notification-card__body">{ msg }</p>
                                }
                                <button class="btn btn--ghost btn--small"
                                        onclick={Callback::from(move |_| dis2.emit(nid))}>
                                    { "Dismiss" }
                                </button>
                            </li>
                        }
                    }) }
                </ul>
            }
        }
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Inbox" }</h1>
                <div class="page__actions">
                    <button class="btn btn--secondary" onclick={ack_all}>{ "Mark all read" }</button>
                </div>
            </header>
            <div class="page__body">
                <div class="tabs">
                    <button class={tab_class(&StatusFilter::Unread)}
                            onclick={set_filter(StatusFilter::Unread)}>
                        { StatusFilter::Unread.label() }
                    </button>
                    <button class={tab_class(&StatusFilter::Queued)}
                            onclick={set_filter(StatusFilter::Queued)}>
                        { StatusFilter::Queued.label() }
                    </button>
                    <button class={tab_class(&StatusFilter::All)}
                            onclick={set_filter(StatusFilter::All)}>
                        { StatusFilter::All.label() }
                    </button>
                </div>
                { content }
            </div>
        </div>
    }
}
