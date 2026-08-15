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
            ("unresolved-reference", "error", 69),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 1),
            ("single-assignment", "error", 1),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.jsonld",
        counts: &[("grounding-failed", "error", 8)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/SeparateWithAFMS.jsonld",
        counts: &[("grounding-failed", "error", 10)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/SeparateWithDP.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 17),
            ("grounding-failed", "error", 11),
            ("single-assignment", "error", 2),
        ],
        duplicate_triples: 2,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.jsonld",
        counts: &[("grounding-failed", "error", 4)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/ReturnFan.jsonld",
        counts: &[("conditional-guard-unknown-parameter", "error", 3)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 132),
            ("conditional-guard-unsupported", "error", 1),
            ("grounding-failed", "error", 2),
            ("single-assignment", "error", 16),
        ],
        duplicate_triples: 58,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 3),
            ("grounding-failed", "error", 5),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.jsonld",
        counts: &[
            ("grounding-failed", "error", 4),
            ("non-subset-construct", "error", 26),
            ("single-assignment", "error", 4),
            ("undriven-boundary-output", "warning", 4),
            ("unresolved-reference", "error", 16),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 9),
            ("grounding-failed", "error", 6),
            ("single-assignment", "error", 1),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 3),
            ("grounding-failed", "error", 4),
            ("non-subset-construct", "error", 15),
            ("single-assignment", "error", 2),
            ("undriven-boundary-output", "warning", 3),
            ("unresolved-reference", "error", 10),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.jsonld",
        counts: &[("conditional-guard-unknown-parameter", "error", 29)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.jsonld",
        counts: &[
            ("grounding-failed", "error", 14),
            ("non-subset-construct", "error", 70),
            ("single-assignment", "error", 17),
            ("unresolved-reference", "error", 34),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.jsonld",
        counts: &[("grounding-failed", "error", 2)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.jsonld",
        counts: &[("grounding-failed", "error", 4)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.jsonld",
        counts: &[
            ("class-not-found", "error", 1),
            ("conditional-guard-unknown-parameter", "error", 1),
            ("grounding-failed", "error", 2),
            ("inactive-conditional-node", "error", 3),
            ("single-assignment", "error", 1),
            ("unresolved-reference", "error", 3),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.jsonld",
        counts: &[
            ("class-not-found", "error", 1),
            ("single-assignment", "error", 1),
            ("unresolved-reference", "error", 3),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 139),
            ("grounding-failed", "error", 46),
            ("single-assignment", "error", 2),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/TimeSuppression.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 1),
            ("grounding-failed", "error", 23),
            ("inactive-conditional-node", "error", 2),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Controller.jsonld",
        counts: &[
            ("class-not-found", "error", 9),
            ("conditional-guard-unknown-parameter", "error", 8),
            ("grounding-failed", "error", 8),
            ("undriven-boundary-output", "warning", 13),
            ("unresolved-reference", "error", 64),
        ],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/ActiveAirFlow.jsonld",
        counts: &[("grounding-failed", "error", 2)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Alarms.jsonld",
        counts: &[("grounding-failed", "error", 17)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/Dampers.jsonld",
        counts: &[("grounding-failed", "error", 5)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/CoolingOnly/Subsequences/SystemRequests.jsonld",
        counts: &[("grounding-failed", "error", 9)],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/TerminalUnits/Reheat/Subsequences/Overrides.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ControlLoops.jsonld",
        counts: &[],
        duplicate_triples: 0,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/ThermalZones/ZoneStates.jsonld",
        counts: &[],
        duplicate_triples: 0,
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
            ("grounding-failed", "error", 6),
            ("inactive-conditional-node", "error", 19),
            ("single-assignment", "error", 7),
        ],
        duplicate_triples: 13,
    },
    DocExpectation {
        rel: "Buildings/Controls/OBC/ASHRAE/G36/VentilationZones/Title24/Setpoints.jsonld",
        counts: &[
            ("conditional-guard-unknown-parameter", "error", 28),
            ("grounding-failed", "error", 6),
            ("inactive-conditional-node", "error", 20),
            ("single-assignment", "error", 3),
        ],
        duplicate_triples: 10,
    },
];

pub(crate) const AGGREGATE: CorpusAggregate = CorpusAggregate {
    counts: &[
        ("class-not-found", "error", 18),
        ("conditional-guard-unknown-parameter", "error", 426),
        ("conditional-guard-unsupported", "error", 1),
        ("grounding-failed", "error", 204),
        ("inactive-conditional-node", "error", 44),
        ("malformed-document", "error", 10),
        ("non-subset-construct", "error", 111),
        ("single-assignment", "error", 57),
        ("undriven-boundary-output", "warning", 27),
        ("unresolved-reference", "error", 199),
    ],
    unresolved_reference_messages: &[
        ("boundary-input target not found", 65),
        ("boundary-output source not found", 25),
        ("connection source not found", 58),
        ("connection target not found", 51),
    ],
    inactive_conditional_messages: &[
        ("connection targets an inactive conditional node", 12),
        (
            "inactive conditional node still carries active connections",
            32,
        ),
    ],
    duplicates_by_code: &[("conditional-guard-unknown-parameter", 85)],
    duplicates_total: 85,
};
