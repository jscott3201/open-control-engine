//! The two files merged and held against the facade's catalog.
//!
//! [`Vocabulary::from_parts`] is where the rules between the files live: curation is additive
//! (a curated description exists only where the generated one is empty), every subject a curated
//! entry names exists, every connector and parameter is described after the merge, and the
//! classes, parameters and named-class connectors are exactly the catalog's, in its order.
//!
//! The join is bounded in work as well as in bytes: each curated class indexes its generated
//! class's subjects once ([`SubjectIndex`]), every curated description and note is then one
//! binary search, and the resolved positions are kept, so a view never searches a list.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{CatalogEntry, CatalogPortNaming, ClassMetadata, ClassParameter};

use crate::data::{CuratedClass, CuratedVocabulary, GeneratedClass, GeneratedVocabulary, Side};
use crate::error::VocabError;
use crate::naming::parameter_base_name;
use crate::range::parameter_range;
use crate::view::{ClassLinks, ClassVocab, ConnectorVocab, ParameterVocab, TextOrigin};

/// One class's place in the catalog and in `curated.json`.
#[derive(Clone, Debug)]
struct ClassJoin {
    metadata: &'static ClassMetadata,
    /// The catalog entry, whose rules the parameter ranges are rendered from.
    entry: &'static CatalogEntry,
    /// The catalog parameter behind each generated parameter, in the same order.
    parameters: Vec<&'static ClassParameter>,
    /// Where the class is in `curated.json`, and which of its subjects that file describes.
    curated: CuratedJoin,
}

/// One generated class's place in `curated.json`, resolved once when the files are joined.
#[derive(Clone, Debug, Default)]
struct CuratedJoin {
    /// Index of the class in `curated.json`, when it is curated.
    class: Option<usize>,
    /// For each subject of the generated class (its connectors, then its parameters), the index
    /// of the curated description that names it. Empty when the class has none.
    descriptions: Vec<Option<usize>>,
}

impl CuratedJoin {
    /// The curated description of the subject at `position`, as an index into the curated
    /// class's `descriptions`.
    fn description(&self, position: usize) -> Option<usize> {
        self.descriptions.get(position).copied().flatten()
    }
}

/// What joining the two files cost, in a unit that does not depend on the machine: the name
/// comparisons made while looking curated subjects up. The crate's cost test holds it to
/// `n log n` in the entries of the two documents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct JoinCost {
    /// Comparisons of two connector or parameter names.
    pub(crate) name_comparisons: u64,
}

/// The connectors and parameters of one generated class by name, built once per curated class
/// so that each curated description and note is one binary search, not a scan of the class.
struct SubjectIndex<'a> {
    /// `(name, position among the class's subjects)`, sorted by name. Names are unique:
    /// [`index_generated`] has already refused a repeated one.
    by_name: Vec<(&'a str, usize)>,
    /// The generated description of the subject at each position.
    generated: Vec<&'a str>,
}

impl<'a> SubjectIndex<'a> {
    fn new(class: &'a GeneratedClass, cost: &mut JoinCost) -> Self {
        let generated: Vec<&str> = subjects(class)
            .map(|(_, description)| description)
            .collect();
        let mut by_name: Vec<(&str, usize)> = subjects(class)
            .enumerate()
            .map(|(position, (name, _))| (name, position))
            .collect();
        by_name.sort_unstable_by(|a, b| {
            cost.name_comparisons += 1;
            a.0.cmp(b.0)
        });
        Self { by_name, generated }
    }

    /// The position of `subject` among the class's subjects, when it names one.
    fn position(&self, subject: &str, cost: &mut JoinCost) -> Option<usize> {
        self.by_name
            .binary_search_by(|(name, _)| {
                cost.name_comparisons += 1;
                (*name).cmp(subject)
            })
            .ok()
            .map(|found| self.by_name[found].1)
    }
}

/// The block vocabulary: `generated.json` and `curated.json`, checked against each other and
/// against the facade's catalog.
#[derive(Clone, Debug)]
pub struct Vocabulary {
    generated: GeneratedVocabulary,
    curated: CuratedVocabulary,
    joins: Vec<ClassJoin>,
    by_path: BTreeMap<String, usize>,
}

impl Vocabulary {
    /// Read both files and join them; see [`Self::from_parts`].
    ///
    /// # Errors
    ///
    /// Any [`VocabError`]: the single-file refusals of [`GeneratedVocabulary::from_json`] and
    /// [`CuratedVocabulary::from_json`], then those of [`Self::from_parts`]. Never panics.
    pub fn from_json(generated: &str, curated: &str) -> Result<Self, VocabError> {
        Self::from_parts(
            GeneratedVocabulary::from_json(generated)?,
            CuratedVocabulary::from_json(curated)?,
        )
    }

    /// Join the two files and hold them against `oce_api::catalog()`.
    ///
    /// The work is `n log n` in the entries of the two files: no curated entry scans its class.
    ///
    /// # Errors
    ///
    /// In this order, the first that applies:
    ///
    /// - [`VocabError::DuplicateClass`] or [`VocabError::DuplicateSubject`] in `generated`;
    /// - for `curated`: [`VocabError::UnknownClass`], [`VocabError::DuplicateClass`],
    ///   [`VocabError::CuratedOutOfOrder`], [`VocabError::EmptyCuratedClass`],
    ///   [`VocabError::UnknownSubject`], [`VocabError::DuplicateSubject`] and
    ///   [`VocabError::CuratedReplacesGenerated`] (curation is additive in v1);
    /// - [`VocabError::Undescribed`] for a connector or parameter neither file describes;
    /// - [`VocabError::NotInCatalog`] or [`VocabError::MissingFromVocabulary`] where the classes,
    ///   a class's parameters or a named class's connectors are not exactly the catalog's, in
    ///   its order. The connectors of the width-driven and positional classes have no fixed
    ///   catalog names and are checked by the crate's tests at widths 1, 2 and the maximum.
    ///
    /// Never panics.
    pub fn from_parts(
        generated: GeneratedVocabulary,
        curated: CuratedVocabulary,
    ) -> Result<Self, VocabError> {
        Self::from_parts_costed(generated, curated).0
    }

    /// [`Self::from_parts`], with what the curated join cost. The cost is reported for a refusal
    /// too: a hostile pair is refused, and the work done before the refusal is what is bounded.
    pub(crate) fn from_parts_costed(
        generated: GeneratedVocabulary,
        curated: CuratedVocabulary,
    ) -> (Result<Self, VocabError>, JoinCost) {
        let mut cost = JoinCost::default();
        let joined = Self::join(generated, curated, &mut cost);
        (joined, cost)
    }

    fn join(
        generated: GeneratedVocabulary,
        curated: CuratedVocabulary,
        cost: &mut JoinCost,
    ) -> Result<Self, VocabError> {
        let by_path = index_generated(&generated)?;
        let curated_of = join_curated(&generated, &curated, &by_path, cost)?;
        for (class, join) in generated.classes.iter().zip(&curated_of) {
            for (position, (name, description)) in subjects(class).enumerate() {
                if description.is_empty() && join.description(position).is_none() {
                    return Err(VocabError::Undescribed {
                        class: class.class_path.clone(),
                        subject: name.to_owned(),
                    });
                }
            }
        }
        let joins = join_catalog(&generated, curated_of)?;
        Ok(Self {
            generated,
            curated,
            joins,
            by_path,
        })
    }

    /// `generated.json` as data.
    #[must_use]
    pub fn generated(&self) -> &GeneratedVocabulary {
        &self.generated
    }

    /// `curated.json` as data, each entry's `evidence` included. `evidence` is for reviewers:
    /// a surface that serves agents prints the merged view ([`Self::class`]) and a note's
    /// `kind`, `subject` and `text` only.
    #[must_use]
    pub fn curated(&self) -> &CuratedVocabulary {
        &self.curated
    }

    /// The vocabulary of `class_path` (relative to `Buildings.Controls.OBC.`), joined with the
    /// catalog. `None` for a path that is not an authorable catalog class: an unknown one, or
    /// one of the engine-reserved lowering identities.
    #[must_use]
    pub fn class(&self, class_path: &str) -> Option<ClassVocab<'_>> {
        self.by_path.get(class_path).map(|&index| self.view(index))
    }

    /// Every class's vocabulary, in catalog order.
    pub fn classes(&self) -> impl Iterator<Item = ClassVocab<'_>> {
        (0..self.generated.classes.len()).map(|index| self.view(index))
    }

    fn view(&self, index: usize) -> ClassVocab<'_> {
        let class = &self.generated.classes[index];
        let join = &self.joins[index];
        let curated = join.curated.class.map(|index| &self.curated.classes[index]);
        let merged = |position, generated| merged_text(&join.curated, curated, position, generated);
        let connector_count = class.connectors.len();
        let connectors = class.connectors.iter().enumerate();
        let parameters = class.parameters.iter().zip(&join.parameters).enumerate();
        ClassVocab {
            class_path: &class.class_path,
            description: join.metadata.description,
            upstream: class.upstream,
            links: class.upstream.then(|| {
                ClassLinks::new(&class.class_path, &self.generated.source.buildings_commit)
            }),
            connectors: connectors
                .map(|(position, connector)| {
                    let (description, origin) = merged(position, &connector.description);
                    ConnectorVocab {
                        name: &connector.name,
                        side: connector.side,
                        dims: connector.dims,
                        description,
                        origin,
                        unit: &connector.unit,
                        quantity: &connector.quantity,
                    }
                })
                .collect(),
            parameters: parameters
                .map(|(position, (parameter, catalog))| {
                    let (description, origin) =
                        merged(connector_count + position, &parameter.description);
                    ParameterVocab {
                        name: &parameter.name,
                        catalog,
                        description,
                        origin,
                        unit: &parameter.unit,
                        quantity: &parameter.quantity,
                        range: parameter_range(&join.entry.param_rules, &parameter.name),
                    }
                })
                .collect(),
            notes: curated.map_or(&[], |class| &class.notes),
        }
    }
}

/// Every connector and parameter of a generated class as `(name, generated description)`:
/// the connectors, then the parameters, each in file order.
fn subjects(class: &GeneratedClass) -> impl Iterator<Item = (&str, &str)> {
    let connectors = class
        .connectors
        .iter()
        .map(|c| (c.name.as_str(), c.description.as_str()));
    let parameters = class
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.description.as_str()));
    connectors.chain(parameters)
}

/// The description a reader gets for the subject at `position` (the class's connectors, then
/// its parameters): the generated one, or the curated one where that is empty.
fn merged_text<'a>(
    join: &CuratedJoin,
    curated: Option<&'a CuratedClass>,
    position: usize,
    generated: &'a str,
) -> (&'a str, TextOrigin) {
    let curated_text = join
        .description(position)
        .and_then(|at| curated?.descriptions.get(at));
    match curated_text {
        Some(description) if generated.is_empty() => {
            (description.text.as_str(), TextOrigin::Curated)
        }
        _ => (generated, TextOrigin::Generated),
    }
}

/// Class path to index, refusing a repeated class or a repeated name inside one.
fn index_generated(generated: &GeneratedVocabulary) -> Result<BTreeMap<String, usize>, VocabError> {
    let mut by_path = BTreeMap::new();
    for (index, class) in generated.classes.iter().enumerate() {
        if by_path.insert(class.class_path.clone(), index).is_some() {
            return Err(VocabError::DuplicateClass {
                class: class.class_path.clone(),
            });
        }
        let mut names = BTreeSet::new();
        for (name, _) in subjects(class) {
            if !names.insert(name) {
                return Err(VocabError::DuplicateSubject {
                    class: class.class_path.clone(),
                    subject: name.to_owned(),
                });
            }
        }
    }
    Ok(by_path)
}

/// For each generated class, its place in `curated.json`, after checking every curated entry.
///
/// Each curated class indexes its generated class's subjects once ([`SubjectIndex`]), so the
/// work is `n log n` in the entries of the two files, not their product.
fn join_curated(
    generated: &GeneratedVocabulary,
    curated: &CuratedVocabulary,
    by_path: &BTreeMap<String, usize>,
    cost: &mut JoinCost,
) -> Result<Vec<CuratedJoin>, VocabError> {
    let mut curated_of = vec![CuratedJoin::default(); generated.classes.len()];
    let mut last: Option<usize> = None;
    for (curated_index, class) in curated.classes.iter().enumerate() {
        let path = || class.class_path.clone();
        let Some(&index) = by_path.get(&class.class_path) else {
            return Err(VocabError::UnknownClass { class: path() });
        };
        if curated_of[index].class.is_some() {
            return Err(VocabError::DuplicateClass { class: path() });
        }
        if last.is_some_and(|last| index < last) {
            return Err(VocabError::CuratedOutOfOrder { class: path() });
        }
        last = Some(index);
        curated_of[index].class = Some(curated_index);
        if class.descriptions.is_empty() && class.notes.is_empty() {
            return Err(VocabError::EmptyCuratedClass { class: path() });
        }
        let subjects = SubjectIndex::new(&generated.classes[index], cost);
        let mut described = vec![None; subjects.generated.len()];
        for (at, description) in class.descriptions.iter().enumerate() {
            let subject = || description.subject.clone();
            let Some(position) = subjects.position(&description.subject, cost) else {
                return Err(VocabError::UnknownSubject {
                    class: path(),
                    subject: subject(),
                });
            };
            if described[position].is_some() {
                return Err(VocabError::DuplicateSubject {
                    class: path(),
                    subject: subject(),
                });
            }
            described[position] = Some(at);
            if !subjects.generated[position].is_empty() {
                return Err(VocabError::CuratedReplacesGenerated {
                    class: path(),
                    subject: subject(),
                });
            }
        }
        for note in &class.notes {
            if !note.subject.is_empty() && subjects.position(&note.subject, cost).is_none() {
                return Err(VocabError::UnknownSubject {
                    class: path(),
                    subject: note.subject.clone(),
                });
            }
        }
        curated_of[index].descriptions = described;
    }
    Ok(curated_of)
}

/// Require `vocabulary` to be exactly `catalog`, in order, reporting the first name that is on
/// one side only (or, when both have it, the vocabulary name that is out of order).
fn same_names(class: &str, vocabulary: &[&str], catalog: &[&str]) -> Result<(), VocabError> {
    for position in 0..vocabulary.len().max(catalog.len()) {
        let (ours, theirs) = (vocabulary.get(position), catalog.get(position));
        if ours == theirs {
            continue;
        }
        if let Some(theirs) = theirs
            && !vocabulary.contains(theirs)
        {
            return Err(VocabError::MissingFromVocabulary {
                class: class.to_owned(),
                subject: (*theirs).to_owned(),
            });
        }
        return Err(match ours {
            Some(ours) => VocabError::NotInCatalog {
                class: class.to_owned(),
                subject: (*ours).to_owned(),
            },
            // Unreachable: a catalog name past the vocabulary's end is missing from it, which
            // the branch above reports. Reported the same way rather than assumed.
            None => VocabError::MissingFromVocabulary {
                class: class.to_owned(),
                subject: theirs.map(|name| (*name).to_owned()).unwrap_or_default(),
            },
        });
    }
    Ok(())
}

/// Hold the generated classes against the catalog and collect each class's catalog entries.
fn join_catalog(
    generated: &GeneratedVocabulary,
    curated_of: Vec<CuratedJoin>,
) -> Result<Vec<ClassJoin>, VocabError> {
    let authorable: Vec<_> = oce_api::catalog()
        .iter()
        .filter(|entry| !entry.reserved)
        .collect();
    let ours: Vec<&str> = generated
        .classes
        .iter()
        .map(|class| class.class_path.as_str())
        .collect();
    let theirs: Vec<&str> = authorable.iter().map(|entry| entry.class_path).collect();
    // A class is reported with an empty subject: swap the roles of class and name.
    same_names("", &ours, &theirs).map_err(|error| match error {
        VocabError::NotInCatalog { subject, .. } => VocabError::NotInCatalog {
            class: subject,
            subject: String::new(),
        },
        VocabError::MissingFromVocabulary { subject, .. } => VocabError::MissingFromVocabulary {
            class: subject,
            subject: String::new(),
        },
        other => other,
    })?;
    let mut joins = Vec::with_capacity(generated.classes.len());
    for ((class, entry), curated) in generated.classes.iter().zip(authorable).zip(curated_of) {
        let path = class.class_path.as_str();
        let Some(metadata) = oce_api::class_metadata()
            .iter()
            .find(|metadata| metadata.class_path == path)
        else {
            return Err(VocabError::NotInCatalog {
                class: path.to_owned(),
                subject: String::new(),
            });
        };
        let ours: Vec<&str> = class.parameters.iter().map(|p| p.name.as_str()).collect();
        let theirs: Vec<&str> = metadata
            .parameters
            .iter()
            .map(|parameter| parameter_base_name(parameter.name))
            .collect();
        same_names(path, &ours, &theirs)?;
        let named = entry.naming == CatalogPortNaming::Named;
        if named {
            for (side, ports) in [(Side::Input, &entry.inputs), (Side::Output, &entry.outputs)] {
                let ours: Vec<&str> = class
                    .connectors
                    .iter()
                    .filter(|connector| connector.side == side)
                    .map(|connector| connector.name.as_str())
                    .collect();
                let theirs: Vec<&str> = ports.iter().filter_map(|port| port.name).collect();
                same_names(path, &ours, &theirs)?;
            }
        }
        // Inputs are listed before outputs, and a named class has scalar connectors only.
        let mut seen_output = false;
        for connector in &class.connectors {
            let misplaced = connector.side == Side::Input && seen_output;
            seen_output |= connector.side == Side::Output;
            if misplaced || (named && connector.dims != 0) {
                return Err(VocabError::NotInCatalog {
                    class: path.to_owned(),
                    subject: connector.name.clone(),
                });
            }
        }
        joins.push(ClassJoin {
            metadata,
            entry,
            parameters: metadata.parameters.iter().collect(),
            curated,
        });
    }
    Ok(joins)
}

#[cfg(test)]
#[path = "vocabulary_tests.rs"]
mod tests;
