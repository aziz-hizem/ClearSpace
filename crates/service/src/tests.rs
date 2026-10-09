use std::time::Duration;

use clearspace_scanner::fake::FakeFs;
use clearspace_scanner::source::ListErrorKind;

use super::*;

fn service() -> ScanService<FakeFs> {
    ScanService::new(ScanOptions { threads: 2 })
}

fn fixture() -> FakeFs {
    FakeFs::new(root_path())
        .file("Videos/holiday.mp4", 8_000_000)
        .file("Videos/clip.MOV", 2_000_000)
        .file("Docs/report.pdf", 300_000)
        .file("Docs/notes.txt", 1_000)
        .file("big.iso", 5_000_000)
        .failing("Private", ListErrorKind::AccessDenied)
}

fn root_path() -> PathBuf {
    normalize_root(Path::new("R")).unwrap()
}

fn finished(svc: &ScanService<FakeFs>) -> Arc<Session<FakeFs>> {
    let s = svc.start(root_path().to_str().unwrap(), fixture()).unwrap();
    for _ in 0..500 {
        if s.poll().status != "running" {
            return s;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("scan did not finish");
}

fn by_size(node: u32) -> ChildrenRequest {
    ChildrenRequest {
        node,
        sort: SortKey::Size,
        descending: true,
        offset: 0,
        limit: 100,
    }
}

#[test]
fn progress_reaches_completed_with_totals() {
    let svc = service();
    let s = finished(&svc);
    let p = s.poll();
    assert_eq!(p.status, "completed");
    assert_eq!(p.files, 5);
    assert_eq!(p.bytes, 15_301_000);
    assert_eq!(p.issues, 1);
    assert_eq!(
        s.issues(10)[0].reason,
        "Windows doesn't allow this folder to be read"
    );
}

#[test]
fn children_sorting_and_paging() {
    let svc = service();
    let s = finished(&svc);
    let all = svc.children(s.id, &by_size(0)).unwrap();
    let names: Vec<_> = all.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names[..3], ["Videos", "big.iso", "Docs"]);
    assert_eq!(all.total, 4);
    let by_name = svc
        .children(
            s.id,
            &ChildrenRequest {
                sort: SortKey::Name,
                descending: false,
                ..by_size(0)
            },
        )
        .unwrap();
    let names: Vec<_> = by_name.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["big.iso", "Docs", "Private", "Videos"],
        "case-insensitive"
    );
    let page = svc
        .children(
            s.id,
            &ChildrenRequest {
                offset: 3,
                limit: 10,
                ..by_size(0)
            },
        )
        .unwrap();
    assert_eq!((page.offset, page.rows.len()), (3, 1));
    let past_end = svc
        .children(
            s.id,
            &ChildrenRequest {
                offset: 99,
                ..by_size(0)
            },
        )
        .unwrap();
    assert!(past_end.rows.is_empty());
}

#[test]
fn rows_carry_flags_and_categories() {
    let svc = service();
    let s = finished(&svc);
    let rows = svc.children(s.id, &by_size(0)).unwrap().rows;
    let private = rows.iter().find(|r| r.name == "Private").unwrap();
    assert!(private.is_dir && private.unreadable);
    let iso = rows.iter().find(|r| r.name == "big.iso").unwrap();
    assert_eq!(iso.category, Some("archives"));
    let videos = rows.iter().find(|r| r.name == "Videos").unwrap();
    assert_eq!(
        (videos.files, videos.child_count, videos.category),
        (2, 2, None)
    );
}

#[test]
fn node_info_has_breadcrumbs_and_path() {
    let svc = service();
    let s = finished(&svc);
    let videos = svc.children(s.id, &by_size(0)).unwrap().rows[0].id;
    let holiday = svc.children(s.id, &by_size(videos)).unwrap().rows[0].id;
    let info = svc.node_info(s.id, holiday).unwrap();
    let crumbs: Vec<_> = info.ancestors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(crumbs[1..], ["Videos", "holiday.mp4"]);
    assert_eq!(info.parent, Some(videos));
    assert!(info.path.ends_with("holiday.mp4"));
    assert_eq!(
        svc.path_of(s.id, holiday).unwrap(),
        root_path().join("Videos").join("holiday.mp4")
    );
}

#[test]
fn unknown_ids_are_errors_not_panics() {
    let svc = service();
    let s = finished(&svc);
    assert!(matches!(
        svc.node_info(s.id, 9_999),
        Err(ServiceError::UnknownNode(_))
    ));
    assert!(matches!(
        svc.children(424_242, &by_size(0)),
        Err(ServiceError::UnknownScan(_))
    ));
    assert!(matches!(
        svc.path_of(s.id, u32::MAX),
        Err(ServiceError::UnknownNode(_))
    ));
}

#[test]
fn treemap_and_type_breakdown() {
    let svc = service();
    let s = finished(&svc);
    let req = TreemapRequest {
        node: 0,
        width: 800.0,
        height: 500.0,
        metric: Metric::Logical,
        max_depth: 8,
    };
    let bytes = svc.treemap(s.id, &req).unwrap();
    let count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert!(count >= 5, "folders and files laid out: {count}");
    assert_eq!(bytes.len(), 8 + count * treemap::CELL_BYTES);
    let bad = TreemapRequest { width: 0.0, ..req };
    assert!(matches!(
        svc.treemap(s.id, &bad),
        Err(ServiceError::BadRequest(_))
    ));

    let tb = svc.type_breakdown(s.id, 0).unwrap();
    assert_eq!(tb.categories[0].category, "videos");
    assert_eq!(tb.categories[0].files, 2);
    let mov = tb.extensions.iter().find(|e| e.ext == "mov").unwrap();
    assert_eq!(mov.size, 2_000_000);
}

#[test]
fn queries_work_while_the_scan_is_still_running() {
    let svc = service();
    let root = root_path();
    let slow = clearspace_scanner::fake::wide_tree(root.to_str().unwrap(), 200, 3)
        .slow(Duration::from_millis(2));
    let s = svc.start(root.to_str().unwrap(), slow).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(s.poll().status, "running");
    assert!(svc.children(s.id, &by_size(0)).is_ok());
    let req = TreemapRequest {
        node: 0,
        width: 400.0,
        height: 300.0,
        metric: Metric::Allocated,
        max_depth: 4,
    };
    assert!(svc.treemap(s.id, &req).is_ok());
    s.cancel();
    for _ in 0..500 {
        if s.poll().status != "running" {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(s.poll().status, "cancelled");
    svc.close(s.id);
    assert!(svc.get(s.id).is_err());
}

#[test]
fn empty_root_is_rejected_with_a_plain_message() {
    let svc = service();
    match svc.start("   ", fixture()) {
        Err(ServiceError::BadRequest(m)) => assert!(m.contains("Choose a drive")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn json_shapes_are_camel_case() {
    let svc = service();
    let s = finished(&svc);
    let v = serde_json::to_value(svc.children(s.id, &by_size(0)).unwrap()).unwrap();
    let row = &v["rows"][0];
    for key in [
        "id",
        "name",
        "isDir",
        "isLink",
        "size",
        "alloc",
        "files",
        "mtime",
        "childCount",
        "cloud",
        "hidden",
        "unreadable",
        "hardlink",
        "category",
    ] {
        assert!(row.get(key).is_some(), "missing {key} in {row}");
    }
    let p = serde_json::to_value(s.poll()).unwrap();
    assert!(p.get("elapsedMs").is_some() && p.get("scanId").is_some());
    let e = serde_json::to_value(ServiceError::UnknownNode("x".into())).unwrap();
    assert_eq!(
        e,
        serde_json::json!({"code": "unknownNode", "message": "x"})
    );
    let req: ChildrenRequest = serde_json::from_value(serde_json::json!({
        "node": 0, "sort": "modified", "descending": true, "offset": 0, "limit": 10
    }))
    .unwrap();
    assert_eq!(req.sort, SortKey::Modified);
}

#[test]
fn rows_by_id_skip_unknown_ids() {
    let svc = service();
    let s = finished(&svc);
    let rows = svc.rows(s.id, &[0, 1, 99_999]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, 0);
    let too_many = vec![0u32; 5_001];
    assert!(matches!(
        svc.rows(s.id, &too_many),
        Err(ServiceError::BadRequest(_))
    ));
}
