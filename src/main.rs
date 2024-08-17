use std::fs::File;
use std::io::Write;

use axum_server::tls_rustls::RustlsConfig;
use camino::{Utf8Path, Utf8PathBuf};
use tokio::task::JoinSet;

use bifrost::config;
use bifrost::error::{ApiError, ApiResult};
use bifrost::mdns;
use bifrost::server;
use bifrost::state::AppState;
use bifrost::z2m;

/*
 * Formatter function to output in syslog format. This makes sense when running
 * as a service (where output might go to a log file, or the system journal)
 */
fn syslog_format(
    buf: &mut pretty_env_logger::env_logger::fmt::Formatter,
    record: &log::Record,
) -> std::io::Result<()> {
    writeln!(
        buf,
        "<{}>{}: {}",
        match record.level() {
            log::Level::Error => 3,
            log::Level::Warn => 4,
            log::Level::Info => 6,
            log::Level::Debug => 7,
            log::Level::Trace => 7,
        },
        record.target(),
        record.args()
    )
}

fn init_logging() -> ApiResult<()> {
    /* Try to provide reasonable default filters, when RUST_LOG is not specified */
    const DEFAULT_LOG_FILTERS: &[&str] = &[
        "debug",
        "mdns_sd=off",
        "tower_http::trace::on_request=info",
        "axum::rejection=trace",
    ];

    let log_filters = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILTERS.join(","));

    /* Detect if we need syslog or human-readable formatting */
    if std::env::var("SYSTEMD_EXEC_PID").is_ok_and(|pid| pid == std::process::id().to_string()) {
        Ok(pretty_env_logger::env_logger::builder()
            .format(syslog_format)
            .parse_filters(&log_filters)
            .try_init()?)
    } else {
        Ok(pretty_env_logger::formatted_timed_builder()
            .parse_filters(&log_filters)
            .try_init()?)
    }
}

async fn load_state(
    conffile: &Utf8Path,
    statefile: &Utf8Path,
    certfile: Utf8PathBuf,
) -> ApiResult<(RustlsConfig, AppState)> {
    let config = config::parse(conffile)?;
    log::debug!("Configuration loaded successfully");

    let appstate = AppState::new(config)?;
    if let Ok(fd) = File::open(statefile) {
        log::debug!("Existing state file found, loading..");
        appstate.res.lock().await.read(fd)?;
    } else {
        log::debug!("No state file found, initializing..");
        appstate.res.lock().await.init(&appstate.bridge_id())?;
    }

    log::debug!("Loading certificate from [{certfile}]");
    let config = RustlsConfig::from_pem_file(&certfile, &certfile)
        .await
        .map_err(|e| ApiError::Certificate(certfile, e))?;

    Ok((config, appstate))
}

async fn build_tasks(
    appstate: AppState,
    config: RustlsConfig,
    statefile: Utf8PathBuf,
) -> ApiResult<JoinSet<ApiResult<()>>> {
    let _mdns = mdns::register_mdns(&appstate);

    let mut tasks = JoinSet::new();

    let svc = server::build_service(appstate.clone());

    log::info!("Serving mac [{}]", appstate.mac());

    tasks.spawn(server::http_server(appstate.ip(), svc.clone()));
    tasks.spawn(server::https_server(appstate.ip(), svc, config));
    tasks.spawn(server::config_writer(appstate.res.clone(), statefile));

    for (name, server) in &appstate.z2m_config().servers {
        let client = z2m::Client::new(
            name.clone(),
            server.url.clone(),
            appstate.config(),
            appstate.res.clone(),
        )?;
        tasks.spawn(client.run_forever());
    }

    Ok(tasks)
}

async fn run() -> ApiResult<()> {
    init_logging()?;

    let certfile = Utf8PathBuf::from("cert.pem");
    let conffile = Utf8PathBuf::from("config.yaml");
    let statefile = Utf8PathBuf::from("state.yaml");

    let (config, appstate) = load_state(&conffile, &statefile, certfile).await?;

    let mut tasks = build_tasks(appstate, config, statefile).await?;

    loop {
        match tasks.join_next().await {
            None => break Ok(()),
            Some(Ok(Ok(res))) => log::info!("Worker returned: {res:?}"),
            Some(Ok(Err(res))) => log::error!("Worked task failed: {res:?}"),
            Some(Err(err)) => log::error!("Error spawning from worker: {err:?}"),
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        log::error!("Bifrost error: {err}");
        log::error!("Fatal error encountered, cannot continue.");
    }
}
