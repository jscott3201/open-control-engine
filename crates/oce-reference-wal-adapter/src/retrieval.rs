//! Deterministic in-memory retrieval recipes over persisted point DTOs.
//!
//! The reference adapter has no semantic graph write path yet, so retrieval scans only
//! [`ResolvedModel::points`](oce_store::ResolvedModel::points). Candidate construction first sorts
//! loaded models by `model_id`, then preserves each model's stored point order; this removes
//! `HashMap` iteration from the observable result. All recipe scores are finite, higher-is-better
//! values at the `oce-store` seam. BM25 uses `ln`, which Rust does not specify as correctly rounded
//! across every target, so tests pin fuzzy ranking rather than exact score bits. Cosine retrieval uses
//! `sqrt`, dot products, division, and an affine remap; the tested seed vectors are chosen to be
//! bit-stable across supported targets.

use std::cmp::Ordering;
use std::collections::HashMap;

use oce_store::{
    DomainKey, PointDto, ResolvedModel, RetrievalHit, SemanticQuery, StoreError, StoreResult,
};
use serde_json::Value;

const TAG_QUERY_MALFORMED: &str = "ReferenceWalStore: TagContains query JSON is malformed";
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

struct Candidate<'a> {
    point: &'a PointDto,
}

struct Document<'a> {
    candidate: Candidate<'a>,
    tokens: Vec<String>,
    term_frequency: HashMap<String, usize>,
}

/// Run one closed semantic retrieval recipe over loaded model point DTOs.
///
/// Scores are unitless, finite, and always higher-is-better. `TagContains` performs recursive JSON
/// structural containment and assigns score `1.0` to matches. `FuzzyText` computes BM25 over the point
/// key leaf plus `search_text`, drops zero-score documents, and accumulates contributions in tokenized
/// query order to avoid hidden hash-order-dependent floating-point summation. `Embedding` computes a
/// cosine score remapped to `[0, 1]`; zero-norm or non-finite vectors score exactly `0.0` rather than
/// emitting NaN. Results are sorted by score descending and key ascending, with duplicate-key equal-score
/// ties preserving the deterministic model-id candidate order.
pub(crate) fn retrieve(
    models: &HashMap<DomainKey, ResolvedModel>,
    q: &SemanticQuery,
) -> StoreResult<Vec<RetrievalHit>> {
    let candidates = candidates(models);
    match q {
        SemanticQuery::TagContains { containment_json } => {
            tag_contains(&candidates, containment_json)
        }
        SemanticQuery::FuzzyText { query, k } => Ok(fuzzy_text(&candidates, query, *k)),
        SemanticQuery::Embedding { query, k } => Ok(embedding(&candidates, query, *k)),
    }
}

fn candidates(models: &HashMap<DomainKey, ResolvedModel>) -> Vec<Candidate<'_>> {
    let mut ordered_models: Vec<_> = models.values().collect();
    ordered_models.sort_by(|a, b| a.model_id.as_str().cmp(b.model_id.as_str()));
    let mut out = Vec::new();
    for model in ordered_models {
        for point in &model.points {
            out.push(Candidate { point });
        }
    }
    out
}

fn tag_contains(
    candidates: &[Candidate<'_>],
    containment_json: &str,
) -> StoreResult<Vec<RetrievalHit>> {
    let needle: Value = serde_json::from_str(containment_json)
        .map_err(|_| StoreError::RetrievalUnsupported(TAG_QUERY_MALFORMED))?;
    let mut hits = Vec::new();
    for candidate in candidates {
        let Some(tags_json) = candidate.point.tags_json.as_deref() else {
            continue;
        };
        let Ok(haystack) = serde_json::from_str::<Value>(tags_json) else {
            continue;
        };
        if contains_json(&haystack, &needle) {
            hits.push(hit(candidate.point.key.clone(), 1.0));
        }
    }
    sort_hits(&mut hits);
    Ok(hits)
}

fn fuzzy_text(candidates: &[Candidate<'_>], query: &str, k: usize) -> Vec<RetrievalHit> {
    if candidates.is_empty() || k == 0 {
        return Vec::new();
    }
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    let mut documents = Vec::with_capacity(candidates.len());
    let mut document_frequency: HashMap<String, usize> = HashMap::new();
    let mut total_len = 0usize;

    for candidate in candidates {
        let mut text = name_leaf(candidate.point.key.as_str()).to_owned();
        if let Some(search) = candidate.point.search_text.as_deref() {
            text.push(' ');
            text.push_str(search);
        }
        let tokens = tokenize(&text);
        total_len += tokens.len();
        let term_frequency = term_frequency(&tokens);
        for term in unique_terms_in_order(&tokens) {
            *document_frequency.entry(term).or_insert(0) += 1;
        }
        documents.push(Document {
            candidate: Candidate {
                point: candidate.point,
            },
            tokens,
            term_frequency,
        });
    }

    let doc_count = documents.len();
    let avgdl = total_len as f64 / doc_count as f64;
    let mut hits = Vec::new();
    for document in &documents {
        let score = bm25_score(
            document,
            &query_tokens,
            &document_frequency,
            doc_count,
            avgdl,
        );
        if score > 0.0 {
            hits.push(hit(document.candidate.point.key.clone(), score));
        }
    }
    sort_hits(&mut hits);
    hits.truncate(k);
    hits
}

fn embedding(candidates: &[Candidate<'_>], query: &[f32], k: usize) -> Vec<RetrievalHit> {
    if query.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for candidate in candidates {
        let Some(vector) = candidate.point.embedding.as_deref() else {
            continue;
        };
        if vector.len() != query.len() {
            continue;
        }
        hits.push(hit(
            candidate.point.key.clone(),
            cosine_score(query, vector),
        ));
    }
    sort_hits(&mut hits);
    hits.truncate(k);
    hits
}

fn contains_json(haystack: &Value, needle: &Value) -> bool {
    match (haystack, needle) {
        (Value::Object(haystack), Value::Object(needle)) => needle.iter().all(|(key, value)| {
            haystack
                .get(key)
                .is_some_and(|hay| contains_json(hay, value))
        }),
        (Value::Array(haystack), Value::Array(needle)) => needle
            .iter()
            .all(|needle_value| haystack.iter().any(|hay| contains_json(hay, needle_value))),
        _ => haystack == needle,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn term_frequency(tokens: &[String]) -> HashMap<String, usize> {
    let mut frequencies = HashMap::new();
    for token in tokens {
        *frequencies.entry(token.clone()).or_insert(0) += 1;
    }
    frequencies
}

fn unique_terms_in_order(tokens: &[String]) -> Vec<String> {
    let mut terms = Vec::new();
    for token in tokens {
        if !terms.iter().any(|term| term == token) {
            terms.push(token.clone());
        }
    }
    terms
}

fn bm25_score(
    document: &Document<'_>,
    query_tokens: &[String],
    document_frequency: &HashMap<String, usize>,
    doc_count: usize,
    avgdl: f64,
) -> f64 {
    let dl = document.tokens.len() as f64;
    let length_norm = if avgdl == 0.0 {
        0.0
    } else {
        BM25_B * dl / avgdl
    };
    let mut score = 0.0;
    for term in query_tokens {
        let Some(&tf) = document.term_frequency.get(term) else {
            continue;
        };
        let df = document_frequency.get(term).copied().unwrap_or(0) as f64;
        let idf = (((doc_count as f64 - df + 0.5) / (df + 0.5)) + 1.0).ln();
        let tf = tf as f64;
        let denom = tf + BM25_K1 * (1.0 - BM25_B + length_norm);
        score += idf * (tf * (BM25_K1 + 1.0)) / denom;
    }
    if score.is_finite() { score } else { 0.0 }
}

fn cosine_score(query: &[f32], document: &[f32]) -> f64 {
    let mut dot = 0.0;
    let mut norm_q = 0.0;
    let mut norm_d = 0.0;
    for (&q, &d) in query.iter().zip(document) {
        if !q.is_finite() || !d.is_finite() {
            return 0.0;
        }
        let q = f64::from(q);
        let d = f64::from(d);
        dot += q * d;
        norm_q += q * q;
        norm_d += d * d;
    }
    if norm_q == 0.0 || norm_d == 0.0 {
        return 0.0;
    }
    let denom = norm_q.sqrt() * norm_d.sqrt();
    if denom == 0.0 || !denom.is_finite() {
        return 0.0;
    }
    let cos = dot / denom;
    if !cos.is_finite() {
        return 0.0;
    }
    (0.5 * (cos + 1.0)).clamp(0.0, 1.0)
}

fn hit(key: DomainKey, score: f64) -> RetrievalHit {
    debug_assert!(score.is_finite());
    RetrievalHit { key, score }
}

fn sort_hits(hits: &mut [RetrievalHit]) {
    hits.sort_by(compare_hits);
}

fn compare_hits(a: &RetrievalHit, b: &RetrievalHit) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.key.as_str().cmp(b.key.as_str()))
}

fn name_leaf(key: &str) -> &str {
    key.rsplit('.').next().unwrap_or(key)
}
