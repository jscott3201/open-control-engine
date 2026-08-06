//! GENERATED expectation table for `vendored_corpus_delta` — regenerate with
//! `OCE_BLESS=1 cargo test -p oce-cxf --test vendored_corpus_delta`, then review the
//! diff: every movement here is a declared consequence of a resolver change, never
//! noise. Counts are per document and per `(DiagCode, severity)`;
//! `duplicate_triples` counts occurrences beyond the first of each exact
//! `(code, subject, message)` triple.

/// Exactly how many vendored documents the recursive walk must find.
pub(crate) const VENDORED_DOCUMENT_COUNT: usize = 44;

/// One document's pinned diagnostic surface.
pub(crate) struct DocExpectation {
    /// Path relative to the vendored `cxf/` root, `/`-joined.
    pub(crate) rel: &'static str,
    /// `(diag-code, severity, count)` rows, sorted by code then severity.
    pub(crate) counts: &'static [(&'static str, &'static str, usize)],
    /// Duplicate `(code, subject, message)` occurrences beyond the first.
    pub(crate) duplicate_triples: usize,
}

/// Corpus-wide totals over the same capture.
pub(crate) struct CorpusAggregate {
    /// `(diag-code, severity, corpus-wide count)` rows.
    pub(crate) counts: &'static [(&'static str, &'static str, usize)],
    /// Exact message-class split of `unresolved-reference`.
    pub(crate) unresolved_reference_messages: &'static [(&'static str, usize)],
    /// Exact message-class split of `inactive-conditional-node`.
    pub(crate) inactive_conditional_messages: &'static [(&'static str, usize)],
    /// Duplicate-triple occurrences beyond the first, per diag-code.
    pub(crate) duplicates_by_code: &'static [(&'static str, usize)],
    /// Duplicate-triple occurrences beyond the first, corpus-wide.
    pub(crate) duplicates_total: usize,
}

pub(crate) const EXPECTED: &[DocExpectation] = &[
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.jsonld",
        counts: &[
            ("class-not-found", "error", 7),
            ("conditional-guard-unknown-parameter", "error", 19),
            ("grounding-failed", "error", 6),
            ("undriven-boundary-output", "warning", 7),
            ("unresolved-reference", "error", 96),
        ],
        duplicate_triples: 24,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 1),
            ("malformed-document", "error", 21),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 62),
        ],
        duplicate_triples: 5,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.jsonld",
        counts: &[
            ("malformed-document", "error", 16),
            ("undriven-boundary-output", "warning", 6),
            ("unresolved-reference", "error", 49),
        ],
        duplicate_triples: 5,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/SeparateWithAFMS.jsonld",
        counts: &[
            ("grounding-failed", "error", 1),
            ("malformed-document", "error", 31),
            ("undriven-boundary-output", "warning", 7),
            ("unresolved-reference", "error", 82),
        ],
        duplicate_triples: 7,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/SeparateWithDP.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 17),
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 36),
            ("undriven-boundary-output", "warning", 6),
            ("unresolved-reference", "error", 99),
        ],
        duplicate_triples: 12,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.jsonld",
        counts: &[
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 8),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 22),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/ReturnFan.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 3),
            ("malformed-document", "error", 6),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 19),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 132),
            ("conditional-guard-unsupported", "error", 1),
            ("malformed-document", "error", 94),
            ("undriven-boundary-output", "warning", 17),
            ("unresolved-reference", "error", 270),
        ],
        duplicate_triples: 98,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 3),
            ("grounding-failed", "error", 3),
            ("malformed-document", "error", 15),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 43),
        ],
        duplicate_triples: 4,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.jsonld",
        counts: &[
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 19),
            ("non-subset-construct", "error", 18),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 47),
        ],
        duplicate_triples: 3,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 9),
            ("grounding-failed", "error", 4),
            ("malformed-document", "error", 15),
            ("undriven-boundary-output", "warning", 6),
            ("unresolved-reference", "error", 47),
        ],
        duplicate_triples: 7,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 3),
            ("grounding-failed", "error", 3),
            ("malformed-document", "error", 10),
            ("non-subset-construct", "error", 10),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 23),
        ],
        duplicate_triples: 1,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 29),
            ("malformed-document", "error", 32),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 94),
        ],
        duplicate_triples: 12,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.jsonld",
        counts: &[
            ("malformed-document", "error", 6),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 13),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.jsonld",
        counts: &[
            ("malformed-document", "error", 19),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 51),
        ],
        duplicate_triples: 5,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.jsonld",
        counts: &[
            ("malformed-document", "error", 90),
            ("non-subset-construct", "error", 53),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 242),
        ],
        duplicate_triples: 26,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.jsonld",
        counts: &[
            ("grounding-failed", "error", 1),
            ("malformed-document", "error", 5),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 12),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.jsonld",
        counts: &[
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 21),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 61),
        ],
        duplicate_triples: 8,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.jsonld",
        counts: &[
            ("class-not-found", "error", 1),
            ("conditional-guard-unknown-parameter", "error", 1),
            ("grounding-failed", "error", 1),
            ("inactive-conditional-node", "error", 3),
            ("malformed-document", "error", 21),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 53),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.jsonld",
        counts: &[
            ("malformed-document", "error", 9),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 31),
        ],
        duplicate_triples: 5,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.jsonld",
        counts: &[
            ("class-not-found", "error", 1),
            ("malformed-document", "error", 18),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 50),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 139),
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 146),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 399),
        ],
        duplicate_triples: 55,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/TimeSuppression.jsonld",
        counts: &[
            ("malformed-document", "error", 24),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 62),
        ],
        duplicate_triples: 6,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 1),
            ("grounding-failed", "error", 9),
            ("inactive-conditional-node", "error", 2),
            ("malformed-document", "error", 44),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 112),
        ],
        duplicate_triples: 12,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Controller.jsonld",
        counts: &[
            ("class-not-found", "error", 9),
            ("conditional-guard-unknown-parameter", "error", 8),
            ("grounding-failed", "error", 8),
            ("undriven-boundary-output", "warning", 13),
            ("unresolved-reference", "error", 71),
        ],
        duplicate_triples: 5,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/ActiveAirFlow.jsonld",
        counts: &[
            ("grounding-failed", "error", 1),
            ("malformed-document", "error", 11),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 27),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Alarms.jsonld",
        counts: &[
            ("grounding-failed", "error", 4),
            ("malformed-document", "error", 47),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 116),
        ],
        duplicate_triples: 11,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Dampers.jsonld",
        counts: &[
            ("grounding-failed", "error", 2),
            ("malformed-document", "error", 35),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 97),
        ],
        duplicate_triples: 6,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.jsonld",
        counts: &[
            ("grounding-failed", "error", 3),
            ("malformed-document", "error", 33),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 84),
        ],
        duplicate_triples: 4,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/Overrides.jsonld",
        counts: &[
            ("malformed-document", "error", 11),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 29),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ControlLoops.jsonld",
        counts: &[
            ("malformed-document", "error", 16),
            ("undriven-boundary-output", "warning", 2),
            ("unresolved-reference", "error", 46),
        ],
        duplicate_triples: 4,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ZoneStates.jsonld",
        counts: &[
            ("malformed-document", "error", 13),
            ("undriven-boundary-output", "warning", 1),
            ("unresolved-reference", "error", 35),
        ],
        duplicate_triples: 3,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/ASHRAEClimateZone.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/ControlEconomizer.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/CoolingCoil.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/EnergyStandard.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeStat.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/HeatingCoil.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/OutdoorAirSection.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/PressureControl.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/Title24ClimateZone.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Types/VentilationStandard.jsonld",
        counts: &[("malformed-document", "error", 1)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/VentilationZones/ASHRAE62_1/Setpoints.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 33),
            ("grounding-failed", "error", 3),
            ("inactive-conditional-node", "error", 19),
            ("malformed-document", "error", 32),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 91),
        ],
        duplicate_triples: 27,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/VentilationZones/Title24/Setpoints.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 28),
            ("grounding-failed", "error", 3),
            ("inactive-conditional-node", "error", 20),
            ("malformed-document", "error", 30),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 88),
        ],
        duplicate_triples: 28,
    },
];

pub(crate) const AGGREGATE: CorpusAggregate = CorpusAggregate {
    counts: &[
        ("class-not-found", "error", 18),
        ("conditional-guard-unknown-parameter", "error", 426),
        ("conditional-guard-unsupported", "error", 1),
        ("grounding-failed", "error", 62),
        ("inactive-conditional-node", "error", 44),
        ("malformed-document", "error", 944),
        ("non-subset-construct", "error", 81),
        ("undriven-boundary-output", "warning", 131),
        ("unresolved-reference", "error", 2723),
    ],
    unresolved_reference_messages: &[
        ("boundary-input target not found", 304),
        ("boundary-output source not found", 163),
        ("connection source not found", 1128),
        ("connection target not found", 1128),
    ],
    inactive_conditional_messages: &[
        ("connection targets an inactive conditional node", 12),
        (
            "inactive conditional node still carries active connections",
            32,
        ),
    ],
    duplicates_by_code: &[
        ("conditional-guard-unknown-parameter", 85),
        ("unresolved-reference", 308),
    ],
    duplicates_total: 393,
};
