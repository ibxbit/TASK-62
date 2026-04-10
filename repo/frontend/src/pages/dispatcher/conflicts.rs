/// Dispatcher conflict monitoring page.
///
/// Shows detected trip scheduling conflicts.  Resolved conflicts are
/// filtered out by default; the dispatcher can view all via the backend
/// query parameter.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::TripConflict,
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<TripConflict>), Error(String) }

#[function_component(ConflictsPage)]
pub fn conflicts_page() -> Html {
    let page_state = use_state(|| PageState::Loading);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match ops_service::list_conflicts().await {
                    Ok(conflicts) => ps.set(PageState::Loaded(conflicts)),
                    Err(e)        => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let content = match &*page_state {
        PageState::Loading => html! {
            <div class="loading-state"><div class="spinner"/><p>{ "Checking for conflicts…" }</p></div>
        },
        PageState::Error(e) => html! {
            <div class="error-state"><p class="error-state__message">{ e }</p></div>
        },
        PageState::Loaded(conflicts) if conflicts.is_empty() => html! {
            <div class="empty-state">
                <p class="empty-state__message">{ "No conflicts detected." }</p>
                <p>{ "All trips are running without scheduling issues." }</p>
            </div>
        },
        PageState::Loaded(conflicts) => html! {
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{ "Trip" }</th>
                        <th>{ "Type" }</th>
                        <th>{ "Description" }</th>
                        <th>{ "Detected" }</th>
                        <th>{ "Status" }</th>
                    </tr>
                </thead>
                <tbody>
                    { for conflicts.iter().map(|c| html! {
                        <tr key={c.id.to_string()}>
                            <td class="mono">{ &c.trip_id.to_string()[..8] }{ "…" }</td>
                            <td>{ &c.conflict_type }</td>
                            <td>{ &c.description }</td>
                            <td>{ c.detected_at.format("%Y-%m-%d %H:%M UTC").to_string() }</td>
                            <td>
                                if c.is_resolved {
                                    <span class="badge badge--success">{ "Resolved" }</span>
                                } else {
                                    <span class="badge badge--warning">{ "Open" }</span>
                                }
                            </td>
                        </tr>
                    }) }
                </tbody>
            </table>
        },
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Conflict Monitor" }</h1>
                <div class="page__actions">
                    <button class="btn btn--secondary"
                            onclick={Callback::from(move |_: MouseEvent| reload())}>
                        { "Refresh" }
                    </button>
                </div>
            </header>
            <div class="page__body">{ content }</div>
        </div>
    }
}
