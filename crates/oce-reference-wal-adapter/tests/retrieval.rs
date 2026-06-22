//! Semantic retrieval recipe tests for the reference WAL adapter.

use std::cmp::Ordering as CmpOrdering;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::ReferenceWalStore;
use oce_store::{
    BlockClassDto, BlockInstanceDto, BlockKind, DomainKey, Durable, ModelStore, OcValue,
    PointDirection, PointDto, PointType, PointValueType, RetrievalHit, SemanticQuery,
    SemanticStore, StoreError, TrendInterval,
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

const TAG_TEMP_GOLDEN: &str = "\
ahu.sat.setpoint\t1.0
ahu.sat.temp\t1.0
";

const TAG_INTEGER_NUMBER_GOLDEN: &str = "\
ahu.sat.temp\t1.0
";

const TAG_FLOAT_NUMBER_GOLDEN: &str = "\
ahu.sat.setpoint\t1.0
";

const FUZZY_SUPPLY_KEYS_GOLDEN: &str = "\
ahu.sat.temp
ahu.sat.setpoint
ahu.misc.supply
vav.zone.temp
";

// Cosine bits are pinned because this fixture uses only IEEE-754 correctly-rounded arithmetic
// (mul/add/div/sqrt, no FMA), making the rendered f64 scores portable across arm64 and x86_64.
const EMBEDDING_GOLDEN: &str = "\
ahu.sat.temp\t1.0\t0x3ff0000000000000
ahu.sat.setpoint\t0.9969418670859622\t0x3fefe6f2a164b764
vav.zone.temp\t0.5\t0x3fe0000000000000
ahu.fan.status\t0.0\t0x0000000000000000
ahu.zero.vector\t0.0\t0x0000000000000000
";

const FUZZY_TIE_KEYS_GOLDEN: &str = "\
tie.alpha
tie.beta
";

const DUPLICATE_KEY_TAG_GOLDEN: &str = "\
dup.shared\t1.0
dup.shared\t1.0
";

struct TestDir {
    path: PathBuf,
}

struct PointSpec {
    key: &'static str,
    search_text: Option<&'static str>,
    tags_json: Option<&'static str>,
    embedding: Option<Vec<f32>>,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-retrieval-{}-{}-{name}",
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

fn class() -> BlockClassDto {
    BlockClassDto {
        class_iri: "CDL.Reals.Sources.Constant".to_owned(),
        kind: BlockKind::Elementary,
        is_library: true,
        shape_json: "{}".to_owned(),
    }
}

fn block() -> BlockInstanceDto {
    BlockInstanceDto {
        key: DomainKey::new("retrieval.block"),
        class_iri: "CDL.Reals.Sources.Constant".to_owned(),
        params: vec![("k".to_owned(), OcValue::Real(1.0))],
        config_json: "{}".to_owned(),
    }
}

fn point(spec: PointSpec) -> PointDto {
    PointDto {
        key: DomainKey::new(spec.key),
        owner_block: DomainKey::new("retrieval.block"),
        direction: PointDirection::Out,
        value_type: PointValueType::Real,
        point_type: PointType::Ao,
        hardwired: false,
        trend_interval_s: TrendInterval::OnChange,
        trend_enable: true,
        quantity: None,
        unit: None,
        display_unit: None,
        description: None,
        in_pointlist: true,
        controlled_device: None,
        search_text: spec.search_text.map(str::to_owned),
        tags_json: spec.tags_json.map(str::to_owned),
        embedding: spec.embedding,
    }
}

fn model(id: &str, points: Vec<PointDto>) -> oce_store::ResolvedModel {
    oce_store::ResolvedModel {
        model_id: DomainKey::new(id),
        schema_rev: 1,
        classes: vec![class()],
        blocks: vec![block()],
        points,
        connections: Vec::new(),
        containment: Vec::new(),
    }
}

fn retrieval_model(id: &str) -> oce_store::ResolvedModel {
    model(
        id,
        vec![
            point(PointSpec {
                key: "ahu.sat.temp",
                search_text: Some(
                    "supply supply supply supply supply air air temperature temperature sensor",
                ),
                tags_json: Some(
                    r#"{"equip":{"type":"AHU","markers":["sensor",{"kind":"temp","loop":"supply"}]},"n":1}"#,
                ),
                embedding: Some(vec![1.0, 0.0, 0.0]),
            }),
            point(PointSpec {
                key: "ahu.sat.setpoint",
                search_text: Some("supply air temperature setpoint"),
                tags_json: Some(
                    r#"{"equip":{"type":"AHU","markers":["setpoint",{"kind":"temp","loop":"supply"}]},"n":1.0}"#,
                ),
                embedding: Some(vec![0.9, 0.1, 0.0]),
            }),
            point(PointSpec {
                key: "vav.zone.temp",
                search_text: Some("zone temperature sensor"),
                tags_json: Some(r#"{"equip":{"type":"VAV","markers":[{"kind":"zone"}]},"n":2}"#),
                embedding: Some(vec![0.0, 1.0, 0.0]),
            }),
            point(PointSpec {
                key: "ahu.fan.status",
                search_text: Some("fan status command"),
                tags_json: Some(r#"{"equip":{"type":"AHU","markers":["fan"]}}"#),
                embedding: Some(vec![-1.0, 0.0, 0.0]),
            }),
            point(PointSpec {
                key: "ahu.zero.vector",
                search_text: Some("zero norm vector"),
                tags_json: Some(r#"{"equip":{"markers":["zero"]}}"#),
                embedding: Some(vec![0.0, 0.0, 0.0]),
            }),
            point(PointSpec {
                key: "ahu.bad.tags",
                search_text: Some("corrupt record"),
                tags_json: Some("{not valid json"),
                embedding: Some(vec![1.0, 1.0]),
            }),
            point(PointSpec {
                key: "ahu.misc.supply",
                search_text: None,
                tags_json: None,
                embedding: None,
            }),
            point(PointSpec {
                key: "tie.beta",
                search_text: Some("tie"),
                tags_json: Some(r#"{"tie":true}"#),
                embedding: None,
            }),
            point(PointSpec {
                key: "tie.alpha",
                search_text: Some("tie"),
                tags_json: Some(r#"{"tie":true}"#),
                embedding: None,
            }),
        ],
    )
}

fn duplicate_key_model(id: &str) -> oce_store::ResolvedModel {
    model(
        id,
        vec![point(PointSpec {
            key: "dup.shared",
            search_text: Some("duplicate"),
            tags_json: Some(r#"{"dup":true}"#),
            embedding: None,
        })],
    )
}

fn empty_document_model(id: &str) -> oce_store::ResolvedModel {
    model(
        id,
        vec![point(PointSpec {
            key: "",
            search_text: None,
            tags_json: Some("{}"),
            embedding: Some(vec![1.0]),
        })],
    )
}

fn partial_tag_model(id: &str) -> oce_store::ResolvedModel {
    model(
        id,
        vec![point(PointSpec {
            key: "tag.partial",
            search_text: Some("partial tag fixture"),
            tags_json: Some(r#"{"a":"present"}"#),
            embedding: None,
        })],
    )
}

fn save_retrieval_model(store: &ReferenceWalStore) {
    store
        .save_model(&retrieval_model("model:retrieval"))
        .expect("save retrieval model");
}

fn render_scores(hits: &[RetrievalHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        out.push_str(hit.key.as_str());
        out.push('\t');
        out.push_str(&format!("{:?}", hit.score));
        out.push('\n');
    }
    out
}

fn render_score_bits(hits: &[RetrievalHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        out.push_str(hit.key.as_str());
        out.push('\t');
        out.push_str(&format!("{:?}", hit.score));
        out.push('\t');
        out.push_str(&format!("0x{:016x}", hit.score.to_bits()));
        out.push('\n');
    }
    out
}

fn render_keys(hits: &[RetrievalHit]) -> String {
    let mut out = String::new();
    for hit in hits {
        out.push_str(hit.key.as_str());
        out.push('\n');
    }
    out
}

fn assert_finite_scores(hits: &[RetrievalHit]) {
    assert!(hits.iter().all(|hit| hit.score.is_finite()));
}

fn assert_rank_consistent(hits: &[RetrievalHit]) {
    for window in hits.windows(2) {
        let left = &window[0];
        let right = &window[1];
        let ordering = right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(CmpOrdering::Equal)
            .then_with(|| left.key.as_str().cmp(right.key.as_str()));
        assert!(
            !matches!(ordering, CmpOrdering::Greater),
            "retrieval hits are not in total comparator order: {left:?} before {right:?}"
        );
    }
}

fn retrieve(store: &ReferenceWalStore, query: SemanticQuery) -> Vec<RetrievalHit> {
    store.retrieve(&query).expect("retrieve")
}

#[test]
fn tag_contains_matches_nested_json_and_sorts_hits() {
    let dir = TestDir::new("tag-contains");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    let hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"equip":{"markers":[{"kind":"temp"}]}}"#.to_owned(),
        },
    );
    assert_eq!(render_scores(&hits), TAG_TEMP_GOLDEN);
    assert_finite_scores(&hits);
    assert_rank_consistent(&hits);
}

#[test]
fn tag_contains_matches_json_numbers_by_exact_representation() {
    let dir = TestDir::new("tag-number-representation");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    let integer_hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"n":1}"#.to_owned(),
        },
    );
    assert_eq!(render_scores(&integer_hits), TAG_INTEGER_NUMBER_GOLDEN);

    let float_hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"n":1.0}"#.to_owned(),
        },
    );
    assert_eq!(render_scores(&float_hits), TAG_FLOAT_NUMBER_GOLDEN);
}

#[test]
fn tag_contains_object_needles_require_every_key() {
    let dir = TestDir::new("tag-object-and");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .save_model(&partial_tag_model("model:partial-tag"))
        .expect("save partial-tag model");

    let single_key_hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"a":"present"}"#.to_owned(),
        },
    );
    assert_eq!(render_scores(&single_key_hits), "tag.partial\t1.0\n");

    let two_key_hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"a":"present","b":"missing"}"#.to_owned(),
        },
    );
    assert!(two_key_hits.is_empty());
}

#[test]
fn fuzzy_text_returns_ranked_positive_hits_without_pinning_libm_bits() {
    let dir = TestDir::new("fuzzy-ranked");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    let hits = retrieve(
        &store,
        SemanticQuery::FuzzyText {
            query: "supply air temperature".to_owned(),
            k: 4,
        },
    );
    assert_eq!(render_keys(&hits), FUZZY_SUPPLY_KEYS_GOLDEN);
    assert!(hits.iter().all(|hit| hit.score > 0.0));
    assert_finite_scores(&hits);
    assert_rank_consistent(&hits);
}

#[test]
fn embedding_returns_exact_bit_stable_scores_and_keeps_zero_norm_hits() {
    let dir = TestDir::new("embedding-bits");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    let hits = retrieve(
        &store,
        SemanticQuery::Embedding {
            query: vec![1.0, 0.0, 0.0],
            k: 10,
        },
    );
    assert_eq!(render_score_bits(&hits), EMBEDDING_GOLDEN);
    assert_eq!(
        hits.iter()
            .find(|hit| hit.key.as_str() == "ahu.zero.vector")
            .expect("zero-norm hit")
            .score
            .to_bits(),
        0.0f64.to_bits()
    );
    assert_finite_scores(&hits);
    assert_rank_consistent(&hits);
}

#[test]
fn retrieval_outputs_are_identical_across_rerun_and_reopen() {
    let dir = TestDir::new("rerun-reopen");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    let tag_query = SemanticQuery::TagContains {
        containment_json: r#"{"equip":{"markers":[{"kind":"temp"}]}}"#.to_owned(),
    };
    let fuzzy_query = SemanticQuery::FuzzyText {
        query: "supply air temperature".to_owned(),
        k: 4,
    };
    let embedding_query = SemanticQuery::Embedding {
        query: vec![1.0, 0.0, 0.0],
        k: 10,
    };

    let tag = render_scores(&retrieve(&store, tag_query.clone()));
    let fuzzy = render_keys(&retrieve(&store, fuzzy_query.clone()));
    let embedding = render_score_bits(&retrieve(&store, embedding_query.clone()));
    assert_eq!(render_scores(&retrieve(&store, tag_query.clone())), tag);
    assert_eq!(render_keys(&retrieve(&store, fuzzy_query.clone())), fuzzy);
    assert_eq!(
        render_score_bits(&retrieve(&store, embedding_query.clone())),
        embedding
    );

    store.commit().expect("commit retrieval model");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    assert_eq!(render_scores(&retrieve(&reopened, tag_query)), tag);
    assert_eq!(render_keys(&retrieve(&reopened, fuzzy_query)), fuzzy);
    assert_eq!(
        render_score_bits(&retrieve(&reopened, embedding_query)),
        embedding
    );
}

#[test]
fn retrieval_tie_breaks_equal_scores_by_key_and_keeps_duplicate_keys() {
    let dir = TestDir::new("tie-breaks");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);
    store
        .save_model(&duplicate_key_model("model-z"))
        .expect("save duplicate-key model z");
    store
        .save_model(&duplicate_key_model("model-a"))
        .expect("save duplicate-key model a");

    let fuzzy_tie = retrieve(
        &store,
        SemanticQuery::FuzzyText {
            query: "tie".to_owned(),
            k: 10,
        },
    );
    assert_eq!(render_keys(&fuzzy_tie), FUZZY_TIE_KEYS_GOLDEN);
    assert_rank_consistent(&fuzzy_tie);

    let duplicate_keys = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"dup":true}"#.to_owned(),
        },
    );
    assert_eq!(render_scores(&duplicate_keys), DUPLICATE_KEY_TAG_GOLDEN);
    assert_eq!(
        render_scores(&retrieve(
            &store,
            SemanticQuery::TagContains {
                containment_json: r#"{"dup":true}"#.to_owned(),
            },
        )),
        DUPLICATE_KEY_TAG_GOLDEN
    );
}

#[test]
fn top_k_and_empty_corpus_queries_return_total_results() {
    let empty_dir = TestDir::new("empty-corpus");
    let empty = ReferenceWalStore::open(empty_dir.path()).expect("open empty store");
    assert!(
        retrieve(
            &empty,
            SemanticQuery::TagContains {
                containment_json: "{}".to_owned(),
            },
        )
        .is_empty()
    );
    assert!(
        retrieve(
            &empty,
            SemanticQuery::FuzzyText {
                query: "anything".to_owned(),
                k: 3,
            },
        )
        .is_empty()
    );
    assert!(
        retrieve(
            &empty,
            SemanticQuery::Embedding {
                query: vec![1.0],
                k: 3,
            },
        )
        .is_empty()
    );

    let dir = TestDir::new("top-k");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);
    assert!(
        retrieve(
            &store,
            SemanticQuery::FuzzyText {
                query: "supply air temperature".to_owned(),
                k: 0,
            },
        )
        .is_empty()
    );
    assert!(
        retrieve(
            &store,
            SemanticQuery::Embedding {
                query: vec![1.0, 0.0, 0.0],
                k: 0,
            },
        )
        .is_empty()
    );
    assert_eq!(
        render_keys(&retrieve(
            &store,
            SemanticQuery::FuzzyText {
                query: "supply air temperature".to_owned(),
                k: 2,
            },
        )),
        "ahu.sat.temp\nahu.sat.setpoint\n"
    );
    assert_eq!(
        render_keys(&retrieve(
            &store,
            SemanticQuery::Embedding {
                query: vec![1.0, 0.0, 0.0],
                k: 2,
            },
        )),
        "ahu.sat.temp\nahu.sat.setpoint\n"
    );
}

#[test]
fn malformed_tag_query_is_typed_error_and_corrupt_stored_tags_are_skipped() {
    let dir = TestDir::new("malformed-tags");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    save_retrieval_model(&store);

    match store.retrieve(&SemanticQuery::TagContains {
        containment_json: "{not valid query".to_owned(),
    }) {
        Err(StoreError::RetrievalUnsupported(msg)) => {
            assert_eq!(
                msg,
                "ReferenceWalStore: TagContains query JSON is malformed"
            );
        }
        other => panic!(
            "expected malformed TagContains query to be typed RetrievalUnsupported, got {other:?}"
        ),
    }

    let hits = retrieve(
        &store,
        SemanticQuery::TagContains {
            containment_json: r#"{"equip":{"type":"AHU"}}"#.to_owned(),
        },
    );
    assert!(hits.iter().all(|hit| hit.key.as_str() != "ahu.bad.tags"));
    assert_eq!(
        render_keys(&hits),
        "ahu.fan.status\nahu.sat.setpoint\nahu.sat.temp\n"
    );
}

#[test]
fn degenerate_retrieval_inputs_never_emit_nan_or_panic() {
    let dir = TestDir::new("degenerate");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .save_model(&empty_document_model("model:empty-doc"))
        .expect("save empty-document model");

    let fuzzy = retrieve(
        &store,
        SemanticQuery::FuzzyText {
            query: "anything".to_owned(),
            k: 10,
        },
    );
    assert!(fuzzy.is_empty());

    let empty_query = retrieve(
        &store,
        SemanticQuery::FuzzyText {
            query: " ,,, ".to_owned(),
            k: 10,
        },
    );
    assert!(empty_query.is_empty());

    let non_finite_embedding = retrieve(
        &store,
        SemanticQuery::Embedding {
            query: vec![f32::NAN],
            k: 10,
        },
    );
    assert_eq!(
        render_score_bits(&non_finite_embedding),
        "\t0.0\t0x0000000000000000\n"
    );
    assert_finite_scores(&non_finite_embedding);
}
