//! The two files merged and held against the facade's catalog.
//!
//! [`Vocabulary::from_parts`] is where the rules between the files live: curation is additive
//! (a curated description exists only where the generated one is empty), every subject a curated
//! entry names exists, every connector and parameter is described after the merge, and the
//! classes, parameters and named-class connectors are exactly the catalog's, in its order.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{CatalogPortNaming, ClassMetadata, ClassParameter};

use crate::data::{CuratedClass, CuratedVocabulary, GeneratedClass, GeneratedVocabulary, Side};
use crate::error::VocabError;
use crate::naming::parameter_base_name;
use crate::view::{ClassLinks, ClassVocab, ConnectorVocab, ParameterVocab, TextOrigin};

/// One class's place in the catalog and in `curated.json`.
#[derive(Clone, Debug)]
struct ClassJoin {
    metadata: &'static ClassMetadata,
    /// The catalog parameter behind each generated parameter, in the same order.
    parameters: Vec<&'static ClassParameter>,
    /// Index of the class in `curated.json`, when it is curated.
    curated: Option<usize>,
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
        let by_path = index_generated(&generated)?;
        let curated_of = join_curated(&generated, &curated, &by_path)?;
        for (class, curated_index) in generated.classes.iter().zip(&curated_of) {
            let curated_class = curated_index.map(|index| &curated.classes[index]);
            for (name, description) in subjects(class) {
                if description.is_empty() && curated_text(curated_class, name).is_none() {
                    return Err(VocabError::Undescribed {
                        class: class.class_path.clone(),
                        subject: name.to_owned(),
                    });
                }
            }
        }
        let joins = join_catalog(&generated, &curated_of)?;
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

    /// `curated.json` as data.
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
        let curated = join.curated.map(|index| &self.curated.classes[index]);
        ClassVocab {
            class_path: &class.class_path,
            description: join.metadata.description,
            upstream: class.upstream,
            links: class.upstream.then(|| {
                ClassLinks::new(&class.class_path, &self.generated.source.buildings_commit)
            }),
            connectors: class
                .connectors
                .iter()
                .map(|connector| {
                    let (description, origin) =
                        merged_text(curated, &connector.name, &connector.description);
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
            parameters: class
                .parameters
                .iter()
                .zip(&join.parameters)
                .map(|(parameter, catalog)| {
                    let (description, origin) =
                        merged_text(curated, &parameter.name, &parameter.description);
                    ParameterVocab {
                        name: &parameter.name,
                        catalog,
                        description,
                        origin,
                        unit: &parameter.unit,
                        quantity: &parameter.quantity,
                    }
                })
                .collect(),
            notes: curated.map_or(&[], |class| &class.notes),
        }
    }
}

/// Every connector and parameter of a generated class as `(name, generated description)`.
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

/// The curated description of `subject`, if the class has one.
fn curated_text<'a>(curated: Option<&'a CuratedClass>, subject: &str) -> Option<&'a str> {
    curated?
        .descriptions
        .iter()
        .find(|description| description.subject == subject)
        .map(|description| description.text.as_str())
}

/// The description a reader gets: the generated one, or the curated one where that is empty.
fn merged_text<'a>(
    curated: Option<&'a CuratedClass>,
    subject: &str,
    generated: &'a str,
) -> (&'a str, TextOrigin) {
    if generated.is_empty()
        && let Some(text) = curated_text(curated, subject)
    {
        return (text, TextOrigin::Curated);
    }
    (generated, TextOrigin::Generated)
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

/// For each generated class, its index in `curated.json`, after checking every curated entry.
fn join_curated(
    generated: &GeneratedVocabulary,
    curated: &CuratedVocabulary,
    by_path: &BTreeMap<String, usize>,
) -> Result<Vec<Option<usize>>, VocabError> {
    let mut curated_of = vec![None; generated.classes.len()];
    let mut last: Option<usize> = None;
    for (curated_index, class) in curated.classes.iter().enumerate() {
        let path = || class.class_path.clone();
        let Some(&index) = by_path.get(&class.class_path) else {
            return Err(VocabError::UnknownClass { class: path() });
        };
        if curated_of[index].is_some() {
            return Err(VocabError::DuplicateClass { class: path() });
        }
        if last.is_some_and(|last| index < last) {
            return Err(VocabError::CuratedOutOfOrder { class: path() });
        }
        last = Some(index);
        curated_of[index] = Some(curated_index);
        if class.descriptions.is_empty() && class.notes.is_empty() {
            return Err(VocabError::EmptyCuratedClass { class: path() });
        }
        let generated_class = &generated.classes[index];
        let generated_text = |subject: &str| {
            subjects(generated_class)
                .find(|(name, _)| *name == subject)
                .map(|(_, description)| description)
        };
        let mut described = BTreeSet::new();
        for description in &class.descriptions {
            let subject = || description.subject.clone();
            let Some(generated_description) = generated_text(&description.subject) else {
                return Err(VocabError::UnknownSubject {
                    class: path(),
                    subject: subject(),
                });
            };
            if !described.insert(description.subject.as_str()) {
                return Err(VocabError::DuplicateSubject {
                    class: path(),
                    subject: subject(),
                });
            }
            if !generated_description.is_empty() {
                return Err(VocabError::CuratedReplacesGenerated {
                    class: path(),
                    subject: subject(),
                });
            }
        }
        for note in &class.notes {
            if !note.subject.is_empty() && generated_text(&note.subject).is_none() {
                return Err(VocabError::UnknownSubject {
                    class: path(),
                    subject: note.subject.clone(),
                });
            }
        }
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
    curated_of: &[Option<usize>],
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
            parameters: metadata.parameters.iter().collect(),
            curated: *curated,
        });
    }
    Ok(joins)
}

#[cfg(test)]
mod tests {
    use super::same_names;
    use crate::VocabError;

    fn missing(subject: &str) -> VocabError {
        VocabError::MissingFromVocabulary {
            class: "C".to_owned(),
            subject: subject.to_owned(),
        }
    }

    fn extra(subject: &str) -> VocabError {
        VocabError::NotInCatalog {
            class: "C".to_owned(),
            subject: subject.to_owned(),
        }
    }

    #[test]
    fn equal_lists_pass_and_two_empty_lists_pass() {
        assert_eq!(same_names("C", &["a", "b"], &["a", "b"]), Ok(()));
        assert_eq!(same_names("C", &[], &[]), Ok(()));
    }

    #[test]
    fn a_name_on_one_side_only_is_reported_from_that_side() {
        assert_eq!(same_names("C", &["a"], &["a", "b"]), Err(missing("b")));
        assert_eq!(same_names("C", &[], &["a"]), Err(missing("a")));
        assert_eq!(same_names("C", &["a", "x"], &["a"]), Err(extra("x")));
        assert_eq!(same_names("C", &["x"], &[]), Err(extra("x")));
        // A rename is a catalog name the vocabulary lacks.
        assert_eq!(
            same_names("C", &["a", "old"], &["a", "new"]),
            Err(missing("new"))
        );
    }

    #[test]
    fn the_same_names_in_another_order_are_refused() {
        assert_eq!(same_names("C", &["b", "a"], &["a", "b"]), Err(extra("b")));
    }
}
