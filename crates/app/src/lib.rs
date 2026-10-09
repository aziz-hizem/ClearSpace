//! The ClearSpace desktop app: a thin Tauri shell over `clearspace-service`.
//!
//! Every command here is read-only. Commands identify items by `(scan id, node id)` and the
//! service resolves paths itself; the UI never hands us a path to act on (the only path it
//! sends is the root it wants *scanned*). Deletion arrives in milestone (e) through
//! `clearspace-cleaner`, behind the review-and-confirm flow.

mod drives;
mod shell;

use std::time::Duration;

use clearspace_scanner::DefaultSource;
use clearspace_service::{
    ChildrenDto, ChildrenRequest, DriveDto, IssueDto, NodeInfoDto, ProgressDto, ScanService,
    ServiceError, TreemapRequest, TypeBreakdownDto,
};
use serde::Serialize;
use tauri::Manager;
use tauri::ipc::{Channel, Response};

type Service = ScanService<DefaultSource>;

/// Streamed to the UI while a scan runs.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
enum ScanEvent {
    Progress(ProgressDto),
    Finished(ProgressDto),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
    log_dir: Option<String>,
}

#[tauri::command]
async fn app_info(app: tauri::AppHandle) -> Result<AppInfo, ServiceError> {
    Ok(AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        log_dir: app
            .path()
            .app_log_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
async fn list_drives() -> Result<Vec<DriveDto>, ServiceError> {
    Ok(drives::list())
}

#[tauri::command]
async fn start_scan(
    svc: tauri::State<'_, Service>,
    root: String,
    on_event: Channel<ScanEvent>,
) -> Result<u32, ServiceError> {
    let session = svc.start(&root, DefaultSource::default())?;
    let id = session.id;
    tracing::info!(scan = id, root = %session.root.display(), "scan started");
    std::thread::Builder::new()
        .name(format!("scan-progress-{id}"))
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let p = session.poll();
                if p.status != "running" {
                    tracing::info!(
                        scan = id,
                        status = p.status,
                        files = p.files,
                        dirs = p.dirs,
                        issues = p.issues,
                        ms = p.elapsed_ms,
                        "scan finished"
                    );
                    let _ = on_event.send(ScanEvent::Finished(p));
                    break;
                }
                if on_event.send(ScanEvent::Progress(p)).is_err() {
                    // The window went away: stop scanning for nobody.
                    session.cancel();
                }
            }
        })
        .map_err(|e| ServiceError::Io(format!("Couldn't start the scan: {e}")))?;
    Ok(id)
}

#[tauri::command]
async fn cancel_scan(svc: tauri::State<'_, Service>, scan_id: u32) -> Result<(), ServiceError> {
    svc.get(scan_id)?.cancel();
    Ok(())
}

#[tauri::command]
async fn close_scan(svc: tauri::State<'_, Service>, scan_id: u32) -> Result<(), ServiceError> {
    svc.close(scan_id);
    Ok(())
}

#[tauri::command]
async fn get_children(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    request: ChildrenRequest,
) -> Result<ChildrenDto, ServiceError> {
    svc.children(scan_id, &request)
}

#[tauri::command]
async fn get_rows(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    nodes: Vec<u32>,
) -> Result<Vec<clearspace_service::NodeRow>, ServiceError> {
    svc.rows(scan_id, &nodes)
}

#[tauri::command]
async fn get_node_info(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    node: u32,
) -> Result<NodeInfoDto, ServiceError> {
    svc.node_info(scan_id, node)
}

#[tauri::command]
async fn get_treemap(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    request: TreemapRequest,
) -> Result<Response, ServiceError> {
    Ok(Response::new(svc.treemap(scan_id, &request)?))
}

#[tauri::command]
async fn get_type_breakdown(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    node: u32,
) -> Result<TypeBreakdownDto, ServiceError> {
    svc.type_breakdown(scan_id, node)
}

#[tauri::command]
async fn get_issues(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    limit: u32,
) -> Result<Vec<IssueDto>, ServiceError> {
    Ok(svc.get(scan_id)?.issues(limit.min(10_000) as usize))
}

#[tauri::command]
async fn open_in_explorer(
    svc: tauri::State<'_, Service>,
    scan_id: u32,
    node: u32,
) -> Result<(), ServiceError> {
    let path = svc.path_of(scan_id, node)?;
    shell::reveal(&path).map_err(ServiceError::Io)
}

fn init_logging(app: &tauri::App) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let dir = app.path().app_log_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("clearspace")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_env("CLEARSPACE_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(writer)
        .try_init()
        .ok()?;
    Some(guard)
}

struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Service::default())
        .setup(|app| {
            let guard = init_logging(app);
            app.manage(LogGuard(guard));
            tracing::info!(version = %app.package_info().version, "app started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_drives,
            start_scan,
            cancel_scan,
            close_scan,
            get_children,
            get_rows,
            get_node_info,
            get_treemap,
            get_type_breakdown,
            get_issues,
            open_in_explorer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ClearSpace");
}
