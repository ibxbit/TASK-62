pub mod compensation;
pub mod gateway;
pub mod handlers;
pub mod import;
pub mod models;
pub mod signature;

use actix_web::web;

use handlers::{
    // Transactions
    create_transaction, list_transactions, get_transaction,
    // Callbacks
    receive_callback, simulate_callback, get_callback,
    // Imports
    upload_import, list_imports, get_import, process_import,
    // Refunds
    create_refund, list_refunds, get_refund, approve_refund, process_refund,
    // Compensation
    list_compensation_jobs, trigger_compensation,
};

/// Register all `/payments` routes onto the Actix-web `ServiceConfig`.
///
/// Route tree:
///
/// ```text
/// POST   /payments/transactions                   create_transaction   (idempotent)
/// GET    /payments/transactions                   list_transactions
/// GET    /payments/transactions/{id}              get_transaction
///
/// POST   /payments/callbacks/simulate             simulate_callback    ← STATIC before /{gw}
/// GET    /payments/callbacks/{id}                 get_callback
/// POST   /payments/callbacks/{gateway}            receive_callback     (signature verified)
///
/// POST   /payments/imports                        upload_import
/// GET    /payments/imports                        list_imports
/// POST   /payments/imports/{id}/process           process_import       ← STATIC before /{id}
/// GET    /payments/imports/{id}                   get_import
///
/// POST   /payments/refunds                        create_refund        (idempotent)
/// GET    /payments/refunds                        list_refunds
/// POST   /payments/refunds/{id}/approve           approve_refund       ← STATIC before /{id}
/// POST   /payments/refunds/{id}/process           process_refund       ← STATIC before /{id}
/// GET    /payments/refunds/{id}                   get_refund
///
/// GET    /payments/compensation/jobs              list_compensation_jobs
/// POST   /payments/compensation/trigger           trigger_compensation
/// ```
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/payments")
            // ---- transactions ----
            .service(
                web::resource("/transactions")
                    .route(web::get().to(list_transactions))
                    .route(web::post().to(create_transaction)),
            )
            .service(
                web::resource("/transactions/{id}")
                    .route(web::get().to(get_transaction)),
            )
            // ---- callbacks — static paths before parameterized ----
            .service(
                web::resource("/callbacks/simulate")
                    .route(web::post().to(simulate_callback)),
            )
            .service(
                web::resource("/callbacks/{id:[0-9a-fA-F\\-]{36}}")
                    .route(web::get().to(get_callback)),
            )
            .service(
                web::resource("/callbacks/{gateway}")
                    .route(web::post().to(receive_callback)),
            )
            // ---- imports — static /process before /{id} ----
            .service(
                web::resource("/imports")
                    .route(web::get().to(list_imports))
                    .route(web::post().to(upload_import)),
            )
            .service(
                web::resource("/imports/{id}/process")
                    .route(web::post().to(process_import)),
            )
            .service(
                web::resource("/imports/{id}")
                    .route(web::get().to(get_import)),
            )
            // ---- refunds — static /approve & /process before /{id} ----
            .service(
                web::resource("/refunds")
                    .route(web::get().to(list_refunds))
                    .route(web::post().to(create_refund)),
            )
            .service(
                web::resource("/refunds/{id}/approve")
                    .route(web::post().to(approve_refund)),
            )
            .service(
                web::resource("/refunds/{id}/process")
                    .route(web::post().to(process_refund)),
            )
            .service(
                web::resource("/refunds/{id}")
                    .route(web::get().to(get_refund)),
            )
            // ---- compensation ----
            .service(
                web::resource("/compensation/jobs")
                    .route(web::get().to(list_compensation_jobs)),
            )
            .service(
                web::resource("/compensation/trigger")
                    .route(web::post().to(trigger_compensation)),
            ),
    );
}
