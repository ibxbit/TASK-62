/// Alerting: Alert list page.
///
/// Displays active alerts with filtering by severity.
/// Supports acknowledge and close actions.
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::alerting_service,
    types::alerting::Alert,
};

#[derive(Clone, PartialEq, Default)]
enum SeverityFilter { #[default] All, Critical, Warning, Info }

impl SeverityFilter {
    fn label(&self) -> &'static str {
        match self { Self::All => "All", Self::Critical => "Critical",
                     Self::Warning => "Warning", Self::Info => "Info" }
    }
    fn matches(&self, alert: &Alert) -> bool {
        match self {
            Self::All      => true,
            Self::Critical => alert.severity == "critical",
            Self::Warning  => alert.severity == "warning",
            Self::Info     => alert.severity == "info",
        }
    }
}

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<Alert>), Error(String) }

#[function_component(AlertsPage)]
pub fn alerts_page() -> Html {
    let page_state = use_state(|| PageState::Loading);
    let filter     = use_state(SeverityFilter::default);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match alerting_service::list_alerts(None).await {
                    Ok(alerts) => ps.set(PageState::Loaded(alerts)),
                    Err(e)     => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    let acknowledge = {
        let reload = reload.clone();
        Callback::from(move |alert_id: uuid::Uuid| {
            let reload = reload.clone();
            spawn_local(async move {
                let _ = alerting_service::acknowledge_alert(alert_id).await;
                reload();
            });
        })
    };

    let close_alert = {
        let reload = reload.clone();
        Callback::from(move |alert_id: uuid::Uuid| {
            let reload = reload.clone();
            spawn_local(async move {
                let _ = alerting_service::close_alert(alert_id).await;
                reload();
            });
        })
    };

    // Severity filter tabs
    let mk_filter_cb = |f: SeverityFilter| {
        let filter = filter.clone();
        Callback::from(move |_: MouseEvent| filter.set(f.clone()))
    };

    let tab_cls = |active: bool| if active { "tab tab--active" } else { "tab" };

    let tabs = html! {
        <div class="tabs">
            <button class={tab_cls(*filter == SeverityFilter::All)}
                    onclick={mk_filter_cb(SeverityFilter::All)}>
                { SeverityFilter::All.label() }
            </button>
            <button class={tab_cls(*filter == SeverityFilter::Critical)}
                    onclick={mk_filter_cb(SeverityFilter::Critical)}>
                { SeverityFilter::Critical.label() }
            </button>
            <button class={tab_cls(*filter == SeverityFilter::Warning)}
                    onclick={mk_filter_cb(SeverityFilter::Warning)}>
                { SeverityFilter::Warning.label() }
            </button>
            <button class={tab_cls(*filter == SeverityFilter::Info)}
                    onclick={mk_filter_cb(SeverityFilter::Info)}>
                { SeverityFilter::Info.label() }
            </button>
        </div>
    };

    let content = match &*page_state {
        PageState::Loading => html! {
            <div class="loading-state"><div class="spinner"/><p>{ "Loading alerts…" }</p></div>
        },
        PageState::Error(e) => html! {
            <div class="error-state"><p class="error-state__message">{ e }</p></div>
        },
        PageState::Loaded(alerts) => {
            let visible: Vec<_> = alerts.iter().filter(|a| filter.matches(a)).collect();
            if visible.is_empty() {
                html! { <div class="empty-state"><p>{ "No alerts matching filter." }</p></div> }
            } else {
                let ack = acknowledge.clone();
                let cls = close_alert.clone();
                html! {
                    <div class="alert-list">
                        { for visible.iter().map(|a| {
                            let aid     = a.id;
                            let ack2    = ack.clone();
                            let cls2    = cls.clone();
                            let is_open = a.is_open();
                            let is_ack  = a.is_acknowledged();
                            html! {
                                <div class={format!("alert-card {}", a.severity_class())}
                                     key={aid.to_string()}>
                                    <div class="alert-card__header">
                                        <span class={format!("badge badge--{}", a.severity)}>
                                            { &a.severity }
                                        </span>
                                        <span class="alert-card__type">{ a.type_label() }</span>
                                        <span class="alert-card__title">{ &a.title }</span>
                                        <span class="alert-card__time">
                                            { a.created_at.format("%Y-%m-%d %H:%M UTC").to_string() }
                                        </span>
                                    </div>
                                    <p class="alert-card__body">{ &a.description }</p>
                                    <div class="alert-card__actions">
                                        if is_open {
                                            <button class="btn btn--small btn--secondary"
                                                    onclick={Callback::from(move |_| ack2.emit(aid))}>
                                                { "Acknowledge" }
                                            </button>
                                        }
                                        if is_open || is_ack {
                                            <button class="btn btn--small btn--danger"
                                                    onclick={Callback::from(move |_| cls2.emit(aid))}>
                                                { "Close" }
                                            </button>
                                        }
                                    </div>
                                </div>
                            }
                        }) }
                    </div>
                }
            }
        }
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Alerts" }</h1>
                <div class="page__actions">
                    <button class="btn btn--secondary"
                            onclick={Callback::from({
                                let r = reload.clone();
                                move |_: MouseEvent| r()
                            })}>
                        { "Refresh" }
                    </button>
                </div>
            </header>
            <div class="page__body">
                { tabs }
                { content }
            </div>
        </div>
    }
}
