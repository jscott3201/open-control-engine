//! Point-list projection tests for the reference WAL adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::ReferenceWalStore;
use oce_store::{
    BlockClassDto, BlockInstanceDto, BlockKind, ConnectionDto, DomainKey, Durable, ModelStore,
    OcValue, PointDirection, PointDto, PointListRow, PointType, PointValueType, ResolvedModel,
    SemanticStore, TrendInterval,
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

const ALL_ROWS_GOLDEN: &str = "\
ahu.bool.di\tdi\tDi\tfalse\tEverySeconds(0)\tahu\tDI zero interval
ahu.bool.do\tdo\tDo\tfalse\tEverySeconds(15)\tahu\tDO output
ahu.mode.op\top\tMode\tfalse\tOnChange\t\t
ahu.pid.u\tu\tAi\ttrue\tOnChange\tahu\tPID input
ahu.pid.y\ty\tAo\tfalse\tEverySeconds(60)\tahu\tPID output
";

const AHU_ROWS_GOLDEN: &str = "\
ahu.bool.di\tdi\tDi\tfalse\tEverySeconds(0)\tahu\tDI zero interval
ahu.bool.do\tdo\tDo\tfalse\tEverySeconds(15)\tahu\tDO output
ahu.pid.u\tu\tAi\ttrue\tOnChange\tahu\tPID input
ahu.pid.y\ty\tAo\tfalse\tEverySeconds(60)\tahu\tPID output
";

const DUPLICATE_KEY_ROWS_GOLDEN: &str = "\
ahu.pid.u\tu\tAi\tfalse\tOnChange\tmodel-a\tfrom model a
ahu.pid.u\tu\tAi\tfalse\tOnChange\tmodel-z\tfrom model z
";

struct TestDir {
    path: PathBuf,
}

struct PointSpec {
    key: &'static str,
    owner_block: &'static str,
    direction: PointDirection,
    value_type: PointValueType,
    point_type: PointType,
    hardwired: bool,
    trend_interval_s: TrendInterval,
    description: Option<&'static str>,
    controlled_device: Option<&'static str>,
    in_pointlist: bool,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-point-list-{}-{}-{name}",
            std::process::id(),
            idx
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn point(spec: PointSpec) -> PointDto {
    PointDto {
        key: DomainKey::new(spec.key),
        owner_block: DomainKey::new(spec.owner_block),
        direction: spec.direction,
        value_type: spec.value_type,
        point_type: spec.point_type,
        hardwired: spec.hardwired,
        trend_interval_s: spec.trend_interval_s,
        trend_enable: true,
        quantity: None,
        unit: None,
        display_unit: None,
        description: spec.description.map(str::to_owned),
        in_pointlist: spec.in_pointlist,
        controlled_device: spec.controlled_device.map(str::to_owned),
        search_text: None,
        tags_json: None,
        embedding: None,
    }
}

fn block(key: &str, class_iri: &str) -> BlockInstanceDto {
    BlockInstanceDto {
        key: DomainKey::new(key),
        class_iri: class_iri.to_owned(),
        params: vec![("k".to_owned(), OcValue::Real(1.0))],
        config_json: "{}".to_owned(),
    }
}

fn point_list_model(id: &str) -> ResolvedModel {
    ResolvedModel {
        model_id: DomainKey::new(id),
        schema_rev: 1,
        classes: vec![
            BlockClassDto {
                class_iri: "CDL.Reals.PID".to_owned(),
                kind: BlockKind::Elementary,
                is_library: true,
                shape_json: "{}".to_owned(),
            },
            BlockClassDto {
                class_iri: "CDL.Integers.Sources.Constant".to_owned(),
                kind: BlockKind::Elementary,
                is_library: true,
                shape_json: "{}".to_owned(),
            },
            BlockClassDto {
                class_iri: "CDL.Logical.Sources.Constant".to_owned(),
                kind: BlockKind::Elementary,
                is_library: true,
                shape_json: "{}".to_owned(),
            },
        ],
        blocks: vec![
            block("ahu.pid", "CDL.Reals.PID"),
            block("ahu.mode", "CDL.Integers.Sources.Constant"),
            block("ahu.bool", "CDL.Logical.Sources.Constant"),
        ],
        points: vec![
            point(PointSpec {
                key: "ahu.bool.di",
                owner_block: "ahu.bool",
                direction: PointDirection::In,
                value_type: PointValueType::Bool,
                point_type: PointType::Di,
                hardwired: false,
                trend_interval_s: TrendInterval::EverySeconds(0),
                description: Some("DI zero interval"),
                controlled_device: Some("ahu"),
                in_pointlist: true,
            }),
            point(PointSpec {
                key: "ahu.bool.do",
                owner_block: "ahu.bool",
                direction: PointDirection::Out,
                value_type: PointValueType::Bool,
                point_type: PointType::Do,
                hardwired: false,
                trend_interval_s: TrendInterval::EverySeconds(15),
                description: Some("DO output"),
                controlled_device: Some("ahu"),
                in_pointlist: true,
            }),
            point(PointSpec {
                key: "ahu.pid.u",
                owner_block: "ahu.pid",
                direction: PointDirection::In,
                value_type: PointValueType::Real,
                point_type: PointType::Ai,
                hardwired: true,
                trend_interval_s: TrendInterval::OnChange,
                description: Some("PID input"),
                controlled_device: Some("ahu"),
                in_pointlist: true,
            }),
            point(PointSpec {
                key: "ahu.pid.y",
                owner_block: "ahu.pid",
                direction: PointDirection::Out,
                value_type: PointValueType::Real,
                point_type: PointType::Ao,
                hardwired: false,
                trend_interval_s: TrendInterval::EverySeconds(60),
                description: Some("PID output"),
                controlled_device: Some("ahu"),
                in_pointlist: true,
            }),
            point(PointSpec {
                key: "ahu.mode.op",
                owner_block: "ahu.mode",
                direction: PointDirection::Out,
                value_type: PointValueType::Int,
                point_type: PointType::Mode,
                hardwired: false,
                trend_interval_s: TrendInterval::OnChange,
                description: None,
                controlled_device: None,
                in_pointlist: true,
            }),
            point(PointSpec {
                key: "ahu.pid.excluded",
                owner_block: "ahu.pid",
                direction: PointDirection::Out,
                value_type: PointValueType::Real,
                point_type: PointType::Ao,
                hardwired: false,
                trend_interval_s: TrendInterval::EverySeconds(5),
                description: Some("excluded"),
                controlled_device: Some("ahu"),
                in_pointlist: false,
            }),
        ],
        connections: vec![ConnectionDto {
            from_point: DomainKey::new("ahu.pid.y"),
            to_point: DomainKey::new("ahu.pid.u"),
        }],
        containment: Vec::new(),
    }
}

fn dotless_point_model(id: &str) -> ResolvedModel {
    let mut model = point_list_model(id);
    model.points = vec![point(PointSpec {
        key: "conn#5",
        owner_block: "ahu.pid",
        direction: PointDirection::Out,
        value_type: PointValueType::Bool,
        point_type: PointType::Do,
        hardwired: false,
        trend_interval_s: TrendInterval::EverySeconds(15),
        description: Some("dotless"),
        controlled_device: Some("ahu"),
        in_pointlist: true,
    })];
    model.connections.clear();
    model
}

fn duplicate_key_model(
    id: &str,
    controlled_device: &'static str,
    description: &'static str,
) -> ResolvedModel {
    let mut model = point_list_model(id);
    model.points = vec![point(PointSpec {
        key: "ahu.pid.u",
        owner_block: "ahu.pid",
        direction: PointDirection::In,
        value_type: PointValueType::Real,
        point_type: PointType::Ai,
        hardwired: false,
        trend_interval_s: TrendInterval::OnChange,
        description: Some(description),
        controlled_device: Some(controlled_device),
        in_pointlist: true,
    })];
    model.connections.clear();
    model
}

fn all_excluded_model(id: &str) -> ResolvedModel {
    let mut model = point_list_model(id);
    model.points = vec![point(PointSpec {
        key: "ahu.pid.excluded",
        owner_block: "ahu.pid",
        direction: PointDirection::Out,
        value_type: PointValueType::Real,
        point_type: PointType::Ao,
        hardwired: false,
        trend_interval_s: TrendInterval::EverySeconds(5),
        description: Some("excluded"),
        controlled_device: Some("ahu"),
        in_pointlist: false,
    })];
    model.connections.clear();
    model
}

fn render_rows(rows: &[PointListRow]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(row.point.as_str());
        out.push('\t');
        out.push_str(&row.name);
        out.push('\t');
        out.push_str(&format!("{:?}", row.point_type));
        out.push('\t');
        out.push_str(if row.hardwired { "true" } else { "false" });
        out.push('\t');
        out.push_str(&format!("{:?}", row.trend_interval_s));
        out.push('\t');
        out.push_str(row.controlled_device.as_deref().unwrap_or(""));
        out.push('\t');
        out.push_str(row.description.as_deref().unwrap_or(""));
        out.push('\n');
    }
    out
}

fn row<'a>(rows: &'a [PointListRow], key: &str) -> &'a PointListRow {
    rows.iter()
        .find(|row| row.point.as_str() == key)
        .expect("point-list row")
}

#[test]
fn point_list_projects_sorted_rows_from_resolved_model_points() {
    let dir = TestDir::new("projection-golden");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let model = point_list_model("model:point-list");
    store.save_model(&model).expect("save point-list model");

    let all_rows = store.point_list(None).expect("list all point rows");
    assert_eq!(render_rows(&all_rows), ALL_ROWS_GOLDEN);
    assert!(
        all_rows
            .iter()
            .all(|row| row.point.as_str() != "ahu.pid.excluded")
    );
    assert_eq!(row(&all_rows, "ahu.mode.op").point_type, PointType::Mode);
    assert_eq!(
        row(&all_rows, "ahu.bool.di").trend_interval_s,
        TrendInterval::EverySeconds(0)
    );
    assert_eq!(row(&all_rows, "ahu.bool.do").point_type, PointType::Do);
    assert_eq!(
        row(&all_rows, "ahu.pid.u").trend_interval_s,
        TrendInterval::OnChange
    );

    let ahu_rows = store.point_list(Some("ahu")).expect("list AHU rows");
    assert_eq!(render_rows(&ahu_rows), AHU_ROWS_GOLDEN);
    assert!(
        store
            .point_list(Some("AHU"))
            .expect("list case-mismatched controlled device")
            .is_empty()
    );
    assert!(
        store
            .point_list(Some("nonexistent"))
            .expect("list missing controlled device")
            .is_empty()
    );

    store.commit().expect("commit point-list model");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    let reopened_rows = reopened.point_list(None).expect("list rows after reopen");
    assert_eq!(render_rows(&reopened_rows), ALL_ROWS_GOLDEN);
}

#[test]
fn point_list_derives_name_from_dotless_key_without_panic() {
    let dir = TestDir::new("dotless-key");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .save_model(&dotless_point_model("model:dotless"))
        .expect("save dotless model");

    let rows = store.point_list(None).expect("list dotless point");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        render_rows(&rows),
        "conn#5\tconn#5\tDo\tfalse\tEverySeconds(15)\tahu\tdotless\n"
    );
}

#[test]
fn point_list_breaks_cross_model_duplicate_key_ties_by_model_id() {
    let dir = TestDir::new("duplicate-key-tie");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .save_model(&duplicate_key_model("model-z", "model-z", "from model z"))
        .expect("save later-sorted model first");
    store
        .save_model(&duplicate_key_model("model-a", "model-a", "from model a"))
        .expect("save earlier-sorted model second");

    let rows = store.point_list(None).expect("list duplicate point keys");
    assert_eq!(render_rows(&rows), DUPLICATE_KEY_ROWS_GOLDEN);
    let rerun_rows = store.point_list(None).expect("list duplicate keys again");
    assert_eq!(render_rows(&rerun_rows), DUPLICATE_KEY_ROWS_GOLDEN);

    store.commit().expect("commit duplicate-key models");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    let reopened_rows = reopened
        .point_list(None)
        .expect("list duplicate point keys after reopen");
    assert_eq!(render_rows(&reopened_rows), DUPLICATE_KEY_ROWS_GOLDEN);
}

#[test]
fn point_list_on_empty_store_is_empty() {
    let dir = TestDir::new("empty");
    let store = ReferenceWalStore::open(dir.path()).expect("open empty store");
    assert!(store.point_list(None).expect("list empty store").is_empty());
}

#[test]
fn point_list_with_only_excluded_points_is_empty() {
    let dir = TestDir::new("all-excluded");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .save_model(&all_excluded_model("model:excluded"))
        .expect("save excluded-only model");

    assert!(
        store
            .point_list(None)
            .expect("list excluded-only model")
            .is_empty()
    );
}
