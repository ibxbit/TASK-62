/// Dispatcher trip adjustment page.
///
/// Lists active trips and allows viewing/adjusting their details.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::Trip,
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<Trip>), Error(String) }

#[function_component(TripsPage)]
pub fn trips_page() -> Html {
    let page_state = use_state(|| PageState::Loading);

    {
        let ps = page_state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match ops_service::list_trips().await {
                    Ok(trips) => ps.set(PageState::Loaded(trips)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
            || ()
        });
    }

    let content = match &*page_state {
        PageState::Loading => html! {
            <div class="loading-state"><div class="spinner"/><p>{ "Loading trips…" }</p></div>
        },
        PageState::Error(e) => html! {
            <div class="error-state"><p class="error-state__message">{ e }</p></div>
        },
        PageState::Loaded(trips) if trips.is_empty() => html! {
            <div class="empty-state"><p>{ "No active trips." }</p></div>
        },
        PageState::Loaded(trips) => html! {
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{ "Trip ID" }</th>
                        <th>{ "Route" }</th>
                        <th>{ "Scheduled" }</th>
                        <th>{ "Status" }</th>
                        <th>{ "Notes" }</th>
                    </tr>
                </thead>
                <tbody>
                    { for trips.iter().map(|t| html! {
                        <tr key={t.id.to_string()}>
                            <td class="mono">{ &t.id.to_string()[..8] }{ "…" }</td>
                            <td class="mono">{ &t.route_id.to_string()[..8] }{ "…" }</td>
                            <td>{ t.scheduled_at.format("%Y-%m-%d %H:%M UTC").to_string() }</td>
                            <td>
                                <span class={format!("badge badge--{}", t.status)}>
                                    { t.status_label() }
                                </span>
                            </td>
                            <td>{ t.notes.as_deref().unwrap_or("—") }</td>
                        </tr>
                    }) }
                </tbody>
            </table>
        },
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Trip Adjustments" }</h1>
            </header>
            <div class="page__body">{ content }</div>
        </div>
    }
}
