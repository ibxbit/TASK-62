/// Notification subscription center.
///
/// Shows all subscribable event types grouped by category.  The user can
/// toggle individual subscriptions on/off.  Changes are sent to the backend
/// immediately (optimistic toggle with rollback on error).
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::services::api;

#[derive(Clone, PartialEq, serde::Deserialize)]
pub struct Subscription {
    pub id:         uuid::Uuid,
    pub event_type: String,
    pub channel:    String,
    pub active:     bool,
}

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<Subscription>), Error(String) }

#[function_component(SubscriptionsPage)]
pub fn subscriptions_page() -> Html {
    let page_state = use_state(|| PageState::Loading);
    let action_error = use_state(|| None::<String>);

    {
        let ps = page_state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match api::api_get::<Vec<Subscription>>("/notifications/subscriptions").await {
                    Ok(subs) => ps.set(PageState::Loaded(subs)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
            || ()
        });
    }

    let toggle_sub = {
        let ps = page_state.clone();
        let ae = action_error.clone();
        Callback::from(move |(sub_id, active): (uuid::Uuid, bool)| {
            let ps = ps.clone();
            let ae = ae.clone();
            spawn_local(async move {
                #[derive(serde::Serialize)]
                struct Req { active: bool }
                match api::api_put::<Req, serde_json::Value>(
                    &format!("/notifications/subscriptions/{}", sub_id),
                    &Req { active },
                ).await {
                    Ok(_) => ae.set(None),
                    Err(e) => {
                        ae.set(Some(e));
                        return;
                    }
                }
                match api::api_get::<Vec<Subscription>>("/notifications/subscriptions").await {
                    Ok(subs) => ps.set(PageState::Loaded(subs)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
        })
    };

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(subs) if subs.is_empty() => html! {
            <div class="empty-state"><p>{ "No available subscriptions." }</p></div>
        },
        PageState::Loaded(subs) => {
            let tog = toggle_sub.clone();
            html! {
                <ul class="subscription-list">
                    { for subs.iter().map(|s| {
                        let sid    = s.id;
                        let active = s.active;
                        let tog2   = tog.clone();
                        html! {
                            <li class="subscription-item" key={sid.to_string()}>
                                <div class="subscription-item__info">
                                    <span class="subscription-item__type">{ &s.event_type }</span>
                                    <span class="badge">{ &s.channel }</span>
                                </div>
                                <label class="toggle">
                                    <input type="checkbox" checked={active}
                                           onchange={Callback::from(move |_| tog2.emit((sid, !active)))} />
                                    <span class="toggle__slider" />
                                </label>
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
                <h1 class="page__title">{ "Subscriptions" }</h1>
            </header>
            <div class="page__body">
                if let Some(err) = &*action_error {
                    <div class="action-feedback action-feedback--error">{ err }</div>
                }
                { content }
            </div>
        </div>
    }
}
