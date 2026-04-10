/// Finance: Refund processing page.
///
/// Lists pending refund requests and allows approving or processing them.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::finance_service,
    types::finance::Refund,
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<Refund>), Error(String) }

#[derive(Clone, PartialEq)]
enum ActionState { Idle, Working, Done(String), Failed(String) }

#[function_component(RefundsPage)]
pub fn refunds_page() -> Html {
    let page_state   = use_state(|| PageState::Loading);
    let action_state = use_state(|| ActionState::Idle);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match finance_service::list_refunds().await {
                    Ok(items) => ps.set(PageState::Loaded(items)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let approve_refund = {
        let ast    = action_state.clone();
        let reload = reload.clone();
        Callback::from(move |refund_id: uuid::Uuid| {
            let ast2   = ast.clone();
            let reload = reload.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match finance_service::approve_refund(refund_id).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Refund approved".to_string())); reload(); }
                    Err(e) => ast2.set(ActionState::Failed(e)),
                }
            });
        })
    };

    let process_refund = {
        let ast    = action_state.clone();
        let reload = reload.clone();
        Callback::from(move |refund_id: uuid::Uuid| {
            let ast2   = ast.clone();
            let reload = reload.clone();
            spawn_local(async move {
                ast2.set(ActionState::Working);
                match finance_service::process_refund(refund_id).await {
                    Ok(_)  => { ast2.set(ActionState::Done("Refund processed".to_string())); reload(); }
                    Err(e) => ast2.set(ActionState::Failed(e)),
                }
            });
        })
    };

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(refunds) if refunds.is_empty() => html! {
            <div class="empty-state"><p>{ "No refund requests." }</p></div>
        },
        PageState::Loaded(refunds) => {
            let appr = approve_refund.clone();
            let proc = process_refund.clone();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Refund ID" }</th>
                            <th>{ "Amount" }</th>
                            <th>{ "Reason" }</th>
                            <th>{ "Status" }</th>
                            <th>{ "Created" }</th>
                            <th>{ "Actions" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for refunds.iter().map(|r| {
                            let rid   = r.id;
                            let appr2 = appr.clone();
                            let proc2 = proc.clone();
                            let can_approve  = r.can_approve();
                            let can_process  = r.can_process();
                            html! {
                                <tr key={rid.to_string()}>
                                    <td class="mono">{ &rid.to_string()[..8] }{ "…" }</td>
                                    <td>{ format!("{:.2}", r.amount) }</td>
                                    <td>{ r.reason.as_deref().unwrap_or("—") }</td>
                                    <td>
                                        <span class={format!("badge badge--{}", r.status)}>
                                            { r.status_label() }
                                        </span>
                                    </td>
                                    <td>{ r.created_at.format("%Y-%m-%d %H:%M UTC").to_string() }</td>
                                    <td class="action-cell">
                                        if can_approve {
                                            <button class="btn btn--small btn--primary"
                                                    onclick={Callback::from(move |_| appr2.emit(rid))}>
                                                { "Approve" }
                                            </button>
                                        }
                                        if can_process {
                                            <button class="btn btn--small btn--secondary"
                                                    onclick={Callback::from(move |_| proc2.emit(rid))}>
                                                { "Process" }
                                            </button>
                                        }
                                    </td>
                                </tr>
                            }
                        }) }
                    </tbody>
                </table>
            }
        }
    };

    let feedback = match &*action_state {
        ActionState::Working   => html! { <div class="action-feedback action-feedback--working">{ "Working…" }</div> },
        ActionState::Done(msg) => html! { <div class="action-feedback action-feedback--success">{ msg }</div> },
        ActionState::Failed(e) => html! { <div class="action-feedback action-feedback--error">{ e }</div> },
        ActionState::Idle      => html! {},
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Refund Processing" }</h1>
            </header>
            { feedback }
            <div class="page__body">{ content }</div>
        </div>
    }
}
